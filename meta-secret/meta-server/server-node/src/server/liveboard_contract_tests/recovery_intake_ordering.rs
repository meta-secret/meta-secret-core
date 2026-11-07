use super::{
    recovery_intake_faults::{Fault, IntakeFaultRepo, Phase},
    recovery_intake_fixture::{IntakeHarness, assert_clean},
    *,
};
use meta_secret_core::{
    crypto::key_pair::KeyPair,
    node::{
        api::{SsRecoveryCompletion, SsRequest},
        common::model::{
            crypto::aead::EncryptedMessage,
            secret::{SecretDistributionData, SsDeclineData, SsDistributionStatus},
        },
        db::{
            descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
            events::shared_secret_event::SsWorkflowObject, repo::generic_db::*,
        },
        security::sign_recovery_completion,
    },
    secret::shared_secret::PlainText,
};
use std::sync::atomic::Ordering;

pub(super) fn response<R: KvLogEventRepo>(
    h: &IntakeHarness<R>,
    approve: bool,
) -> Result<SignedAction> {
    let claim = &h.source.0.value;
    let id = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| &id.distribution_id.receiver == h.base.b.device_id())
        .context("addressed real receiver")?;
    let event = if approve {
        let share = h
            .base
            .b
            .device_creds
            .key_manager()?
            .transport
            .encrypt_string(
                PlainText::from("intake-lifecycle-authorization-fixture"),
                &h.base.a.device_creds.key_manager()?.transport.pk(),
            )?;
        SsWorkflowObject::Recovery(KvLogEvent {
            key: KvKey::from(SsWorkflowDescriptor::Recovery(id)),
            value: SecretDistributionData {
                vault_name: claim.vault_name.clone(),
                claim_id: claim.dist_claim_id.clone(),
                secret_message: EncryptedMessage::CipherShare { share },
            },
        })
    } else {
        SsWorkflowObject::Decline(KvLogEvent {
            key: KvKey::from(SsWorkflowDescriptor::Decline(id)),
            value: SsDeclineData {
                vault_name: claim.vault_name.clone(),
                claim_id: claim.id.clone(),
                receiver_id: h.base.b.device_id().clone(),
            },
        })
    };
    sign_event(
        event.to_generic(),
        h.base.b.device_id().clone(),
        &h.base.b.device_creds.key_manager()?.dsa,
    )
}
async fn blocked_action(kind: u8) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(IntakeFaultRepo::new(inner.clone()));
    let h = IntakeHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    h.seed_prepared().await?;
    repo.arm(Phase::Raw, Fault::Before);
    let before = inner.get_db().await;
    let request = match kind {
        0 => SyncRequest::Write(Box::new(WriteSyncRequest::Event(response(&h, false)?))),
        1 => {
            let p = Arc::new(PersistentObject::new(repo.clone()));
            let event = next_event(
                &p,
                &h.base.a,
                VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.base.removal())),
            )
            .await?;
            SyncRequest::Write(Box::new(WriteSyncRequest::Event(sign_event(
                event.to_generic(),
                h.base.a.device_id().clone(),
                &h.base.a.device_creds.key_manager()?.dsa,
            )?)))
        }
        2 => {
            let id = h
                .source
                .0
                .value
                .recovery_db_ids()
                .into_iter()
                .find(|id| &id.distribution_id.receiver == h.base.b.device_id())
                .context("completion receiver")?;
            let completion = sign_recovery_completion(
                SsRecoveryCompletion {
                    vault_name: h.source.0.value.vault_name.clone(),
                    recovery_id: id,
                    receiver_status: SsDistributionStatus::Sent,
                },
                h.base.a.device_id().clone(),
                &h.base.a.device_creds.key_manager()?.dsa,
            )?;
            SyncRequest::Read(Box::new(ReadSyncRequest::SsRecoveryCompletion(completion)))
        }
        _ => h.tail(),
    };
    let result = server.handle_client_request(request).await;
    assert!(
        inner.get_db().await == before,
        "RED: later action overran Prepared initial source"
    );
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), 0);
    assert!(
        repo.server_recovery_response_store()?
            .load(&h.base.a.vault_name)
            .await?
            .is_none()
    );
    let error = result
        .err()
        .context("RED: failed original intake repair must block this later action")?;
    assert!(
        error
            .to_string()
            .contains("RecoveryRequestIntake.Unavailable")
    );
    assert!(
        repo.hits.load(Ordering::SeqCst) > 0,
        "original intake raw repair must be attempted before later effects"
    );
    Ok(())
}
#[tokio::test]
async fn recovery_intake_failed_original_raw_repair_blocks_receiver_decision() -> Result<()> {
    blocked_action(0).await
}
#[tokio::test]
async fn recovery_intake_failed_original_raw_repair_blocks_topology_request() -> Result<()> {
    blocked_action(1).await
}
#[tokio::test]
async fn recovery_intake_failed_original_raw_repair_blocks_completion_before_status_mutation()
-> Result<()> {
    blocked_action(2).await
}
#[tokio::test]
async fn recovery_intake_failed_original_raw_repair_blocks_healthy_server_tail() -> Result<()> {
    blocked_action(3).await
}

#[tokio::test]
async fn recovery_intake_queued_actual_server_canonical_fault_returns_error_without_raw_tail_advance()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(IntakeFaultRepo::new(inner.clone()));
    let h = IntakeHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    repo.arm(Phase::Canonical, Fault::Before);
    let transfer = server.get_data_transfer();
    let request = SyncRequest::Write(Box::new(WriteSyncRequest::Event(h.signed.clone())));
    let result = tokio::select! {
        run = server.run() => { run?; anyhow::bail!("server loop unexpectedly stopped"); },
        response = transfer.send_request(request) => response?,
    };
    assert!(matches!(result, DataSyncResponse::Error { .. }));
    assert!(repo.hits.load(Ordering::SeqCst) > 0);
    assert!(
        inner.find_one(h.source.obj_id()).await?.is_none(),
        "RED: actual queued handler acknowledged raw before canonical proof"
    );
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn recovery_intake_after_clean_actual_peer_decisions_completion_and_replay_preserve_terminal_state()
-> Result<()> {
    for approve in [false, true] {
        let h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
        let server = h.server().await?;
        send(&server, h.signed.clone()).await?;
        send(&server, response(&h, approve)?).await?;
        if approve {
            let id = h
                .source
                .0
                .value
                .recovery_db_ids()
                .into_iter()
                .find(|id| &id.distribution_id.receiver == h.base.b.device_id())
                .context("completion receiver")?;
            let action = sign_recovery_completion(
                SsRecoveryCompletion {
                    vault_name: h.base.a.vault_name.clone(),
                    recovery_id: id,
                    receiver_status: SsDistributionStatus::Sent,
                },
                h.base.a.device_id().clone(),
                &h.base.a.device_creds.key_manager()?.dsa,
            )?;
            assert_clean(
                &server
                    .handle_client_request(SyncRequest::Read(Box::new(
                        ReadSyncRequest::SsRecoveryCompletion(action),
                    )))
                    .await?,
            )?;
        }
        let settled = h.log().await?;
        assert!(settled.0.value.pending_recovery_request_intake.is_none());
        assert!(send(&server, h.signed.clone()).await.is_err());
        assert!(
            h.log().await? == settled,
            "initial Event replay reset terminal/completed claim"
        );
        let data = server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                SsRequest {
                    sender: h.base.a.user(),
                    ss_log: h.base_log.obj_id(),
                },
            ))))
            .await?;
        assert_clean(&data)?;
    }
    Ok(())
}
