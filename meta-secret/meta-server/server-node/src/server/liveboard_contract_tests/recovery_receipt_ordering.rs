use super::recovery_receipt_faults::{Fault, ReceiptFaultRepo, Step};
use super::recovery_receipt_fixture::ReceiptHarness;
use super::*;
use meta_secret_core::node::db::events::vault::vault_log_event::{
    DeleteSecretDecision, DeleteSecretRequest,
};
use meta_secret_core::node::db::repo::generic_db::FindOneQuery;

async fn event<R: KvLogEventRepo>(
    h: &ReceiptHarness<R>,
    actor: &UserCreds,
    action: VaultActionEvent,
) -> Result<SignedAction> {
    let p = Arc::new(PersistentObject::new(h.repo.clone()));
    let e = next_event(&p, actor, action).await?;
    sign_event(
        e.to_generic(),
        actor.device_id().clone(),
        &actor.device_creds.key_manager()?.dsa,
    )
}

#[tokio::test]
async fn recovery_receipt_unsupported_storage_fails_direct_server_initialization() -> Result<()> {
    let repo = Arc::new(ReceiptFaultRepo::unsupported(Arc::new(
        InMemKvLogEventRepo::default(),
    )));
    let h = ReceiptHarness::new(repo).await?;
    let server = ServerApp::new(h.repo.clone(), h.base.master_key.clone())?;
    assert!(
        server.init().await.is_err(),
        "RED: unsupported persistent server backend must never silently use volatile fallback"
    );
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_unsupported_storage_fails_queued_run_before_accepting_requests()
-> Result<()> {
    let repo = Arc::new(ReceiptFaultRepo::unsupported(Arc::new(
        InMemKvLogEventRepo::default(),
    )));
    let h = ReceiptHarness::new(repo).await?;
    let server = ServerApp::new(h.repo.clone(), h.base.master_key.clone())?;
    let transfer = server.get_data_transfer();
    let request = SyncRequest::Read(Box::new(ReadSyncRequest::LiveboardCapabilities(
        meta_secret_core::node::api::LiveboardCapabilitiesRequest { version: 1 },
    )));
    tokio::select! {
        run = server.run() => assert!(run.is_err(), "unsupported run startup must fail"),
        response = transfer.send_request(request) => { let _ = response?; anyhow::bail!("RED: queued run accepted work without private storage readiness"); }
    }
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_queued_error_is_safe_and_restart_repairs_without_sender_retransmission()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(ReceiptFaultRepo::new(inner));
    let h = ReceiptHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    let packet = h.packet(&h.base.b, true)?;
    let marker = super::recovery_receipt_privacy::cipher_marker(&packet)?;
    repo.arm(Step::Reserve, Fault::LostReply);
    let transfer = server.get_data_transfer();
    let response = tokio::select! {
        run = server.run() => { run?; anyhow::bail!("server loop unexpectedly ended"); },
        response = transfer.send_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(packet)))) => response?,
    };
    assert!(
        matches!(&response, DataSyncResponse::Error { .. }),
        "RED: queued reserve uncertainty must return an error"
    );
    assert!(
        !serde_json::to_string(&response)?.contains(&marker),
        "queued server error leaked ciphertext"
    );
    repo.clear();
    drop(server);
    refresh(&h.server().await?, &h.base.a).await?;
    h.assert_winner(true).await?;
    Ok(())
}

async fn cleanup_blocks_action(delete_secret: bool) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(ReceiptFaultRepo::new(inner.clone()));
    let h = ReceiptHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    repo.arm(Step::Raw, Fault::Before);
    assert!(send(&server, h.packet(&h.base.b, true)?).await.is_err());
    repo.clear();
    let p = PersistentVault::from(Arc::new(PersistentObject::new(repo.clone())));
    let vault = p.get_vault(h.claim.vault_name.clone()).await?.to_data();
    let request = if delete_secret {
        VaultActionRequestEvent::DeleteSecret(DeleteSecretRequest::new(
            member(&h.base.a),
            h.claim.dist_claim_id.pass_id.clone(),
            &vault,
        ))
    } else {
        VaultActionRequestEvent::RemoveMember(h.base.removal())
    };
    let original = event(&h, &h.base.a, VaultActionEvent::Request(request.clone())).await?;
    repo.arm(Step::Remove, Fault::Before);
    assert!(
        send(&server, original.clone()).await.is_err(),
        "RED: membership/tombstone writes must wait for private Recovery cleanup"
    );
    assert!(
        inner.find_one(original.event()?.obj_id()).await?.is_none(),
        "retirement command must not advance its raw tail before cleanup"
    );
    h.assert_winner(true).await?;
    repo.clear();
    send(&server, original).await?;
    assert!(
        inner
            .server_recovery_response_store()?
            .load(&h.claim.vault_name)
            .await?
            .is_none()
    );
    if let VaultActionRequestEvent::DeleteSecret(request) = request {
        let decision = event(
            &h,
            &h.base.c,
            VaultActionEvent::Update(VaultActionUpdateEvent::DeleteSecret(DeleteSecretDecision {
                request,
                sender: member(&h.base.c),
                accepted: true,
            })),
        )
        .await?;
        send(&server, decision).await?;
        let vault = p.get_vault(h.claim.vault_name.clone()).await?.to_data();
        assert!(
            vault
                .deleted_secrets
                .iter()
                .any(|p| p.id == h.claim.dist_claim_id.pass_id.id)
        );
        assert!(!inner.get_db().await.values().any(|e| matches!(e, meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent::SsWorkflow(_))), "ordinary tombstone purge still removes delivered/private recovery material");
    }
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_cleanup_precedes_device_removal_request_and_retirement_boundary()
-> Result<()> {
    cleanup_blocks_action(false).await
}

#[tokio::test]
async fn recovery_receipt_cleanup_precedes_secret_tombstone_and_ciphertext_purge() -> Result<()> {
    cleanup_blocks_action(true).await
}
