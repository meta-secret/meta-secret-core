//! Actual server acknowledgement, local deletion and checked-entry concurrency.
use super::fixtures;
use anyhow::{Context, Result, bail, ensure};
use meta_secret_core::node::{
    api::{DataSyncResponse, ReadSyncRequest, SignedActionPayload, SyncRequest, WriteSyncRequest},
    app::{
        meta_app::{
            client_transfer::MetaClientRequestTransfer,
            meta_client_service::{
                MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
            },
        },
        sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol},
    },
    common::model::{
        ApplicationState, VaultFullInfo, secret::RecoveryClientStatus, user::common::UserDataMember,
    },
    db::{
        events::{generic_log_event::GenericKvLogEvent, shared_secret_event::SsWorkflowObject},
        in_mem_db::InMemKvLogEventRepo,
        objects::persistent_shared_secret::PersistentSharedSecret,
        repo::generic_db::*,
    },
};
use meta_server_node::server::server_app::ServerApp;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
struct ActualServerFault {
    server: Arc<ServerApp<InMemKvLogEventRepo>>,
    ack: AtomicUsize,
    fault_hit: AtomicUsize,
    fail_download: AtomicBool,
}
impl SyncProtocol for ActualServerFault {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let recovery = matches!(&request, SyncRequest::Write(write)
            if matches!(write.as_ref(), WriteSyncRequest::Event(action)
                if matches!(&action.payload, SignedActionPayload::Event(
                    GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_))))));
        if self.fail_download.load(Ordering::SeqCst)
            && self.ack.load(Ordering::SeqCst) > 0
            && matches!(&request, SyncRequest::Read(read)
                if matches!(read.as_ref(), ReadSyncRequest::SsRequest(_)))
        {
            self.fault_hit.fetch_add(1, Ordering::SeqCst);
            bail!("controlled post-ack shared-secret download failure");
        }
        let response = self.server.handle_client_request(request).await?;
        if recovery {
            response.clone().ensure_success()?;
            self.ack.fetch_add(1, Ordering::SeqCst);
        }
        Ok(response)
    }
}
fn service(
    h: &fixtures::Harness,
    p: Arc<ActualServerFault>,
) -> MetaClientService<super::fault_repo::Repo, ActualServerFault> {
    MetaClientService {
        data_transfer: Arc::new(MetaClientDataTransfer {
            dt: MetaClientRequestTransfer::new(),
        }),
        sync_gateway: Arc::new(SyncGateway::new(
            "ack-delete-real-server".into(),
            h.p.clone(),
            p,
            h.master.clone(),
        )),
        state_provider: Arc::new(MetaClientStateProvider::new()),
        p_obj: h.p.clone(),
        device_data: h.approver.device(),
        master_key: h.master.clone(),
    }
}
#[tokio::test]
async fn actual_ack_delete_then_download_failure_requires_fresh_terminal_read() -> Result<()> {
    let mut h = fixtures::Harness::new(3).await?;
    let pass = h.seed_secret("controlled-ack-delete").await?;
    let claim = h
        .vault
        .clone()
        .to_vault_member(UserDataMember::from(h.initiator.user()))?
        .create_recovery_claim(pass);
    PersistentSharedSecret::from(h.p.clone())
        .save_ss_log_event(claim.clone())
        .await?;
    // Actual response executor creates the genuine encrypted source.
    h.service().accept_recover(claim.id.clone()).await?;
    let sources: Vec<_> =
        h.p.repo
            .get_db()
            .await
            .into_values()
            .filter(|event| {
                matches!(
                    event,
                    GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_))
                )
            })
            .collect();
    ensure!(!sources.is_empty(), "real original response required");
    let server_repo = Arc::new(InMemKvLogEventRepo::default());
    // Mirror the existing native producer harness seed policy; no fake ACK/terminal.
    for event in h.p.repo.get_db().await.into_values() {
        if !matches!(
            event,
            GenericKvLogEvent::UserCreds(_)
                | GenericKvLogEvent::DeviceCreds(_)
                | GenericKvLogEvent::LocalRemovalEnvelope(_)
                | GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_))
        ) {
            server_repo.save(event).await?;
        }
    }
    let server = Arc::new(ServerApp::new(server_repo, h.master.clone())?);
    server.init().await?;
    let protocol = Arc::new(ActualServerFault {
        server,
        ack: 0.into(),
        fault_hit: 0.into(),
        fail_download: AtomicBool::new(true),
    });
    let service = service(&h, protocol.clone());
    assert!(service.sync_gateway.sync(h.approver.user()).await.is_err());
    ensure!(
        protocol.ack.load(Ordering::SeqCst) > 0,
        "actual server ACK required"
    );
    ensure!(
        protocol.fault_hit.load(Ordering::SeqCst) > 0,
        "later actual download fault required"
    );
    for source in &sources {
        use meta_secret_core::node::db::events::generic_log_event::ObjIdExtractor;
        ensure!(
            h.p.repo.find_one(source.obj_id()).await?.is_none(),
            "actual gateway must delete exact acknowledged local source"
        );
    }
    protocol.fail_download.store(false, Ordering::SeqCst);
    let prior_acks = protocol.ack.load(Ordering::SeqCst);
    assert!(
        service
            .accept_recover_revalidated(claim.id.clone(), h.approver.user())
            .await
            .is_err(),
        "A checked entry must refuse after B actual ACK/delete and failed download"
    );
    assert_eq!(
        protocol.ack.load(Ordering::SeqCst),
        prior_acks,
        "A must never publish a second response after B was acknowledged"
    );
    for source in &sources {
        use meta_secret_core::node::db::events::generic_log_event::ObjIdExtractor;
        ensure!(
            h.p.repo.find_one(source.obj_id()).await?.is_none(),
            "A must not leave a reminted local response"
        );
    }
    let fresh = service.get_app_state().await?;
    let ApplicationState::Vault(VaultFullInfo::Member(member)) = fresh else {
        bail!("successful canonical read must retain actual member scope")
    };
    let fresh_claim = member
        .ss_claims
        .claims
        .get(&claim.id)
        .context("exact canonical claim")?;
    assert_ne!(
        fresh_claim.client_status,
        Some(RecoveryClientStatus::NeedApprove),
        "BLOCKER: acknowledged response source vanished but fresh canonical read permits another Approve"
    );
    Ok(())
}

#[tokio::test]
async fn actual_waiting_checked_a_refuses_after_owned_b_ack_delete_download_failure() -> Result<()>
{
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
    let mut h = fixtures::Harness::new(3).await?;
    let pass = h.seed_secret("controlled-ack-delete").await?;
    let claim = h.vault.clone().to_vault_member(UserDataMember::from(h.initiator.user()))?
        .create_recovery_claim(pass);
    PersistentSharedSecret::from(h.p.clone()).save_ss_log_event(claim.clone()).await?;
    let server_repo = Arc::new(InMemKvLogEventRepo::default());
    // Mirror the existing native producer harness seed policy; no fake ACK/terminal.
    for event in h.p.repo.get_db().await.into_values() {
        if !matches!(event, GenericKvLogEvent::UserCreds(_)
            | GenericKvLogEvent::DeviceCreds(_)
            | GenericKvLogEvent::LocalRemovalEnvelope(_)
            | GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_)))
        {
            server_repo.save(event).await?;
        }
    }
    let server = Arc::new(ServerApp::new(server_repo, h.master.clone())?);
    server.init().await?;
    let protocol = Arc::new(ActualServerFault { server, ack: 0.into(), fault_hit: 0.into(),
        fail_download: AtomicBool::new(true) });
    let service = service(&h, protocol.clone());
    let b_owner = service.p_obj.client_operation_guard.lock().await;
    let mut a = std::pin::pin!(service.accept_recover_revalidated(claim.id.clone(), h.approver.user()));
    std::future::poll_fn(|cx| {
        use std::future::Future;
        assert!(a.as_mut().poll(cx).is_pending(), "A must wait before B response begins");
        std::task::Poll::Ready(())
    }).await;
    meta_secret_core::node::app::orchestrator::MetaOrchestrator {
        p_obj: service.p_obj.clone(), user_creds: h.approver.clone(),
    }.accept_recover(claim.id.clone()).await?;
    let sources: Vec<_> = h.p.repo.get_db().await.into_values().filter(|event|
        matches!(event, GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_)))).collect();
    ensure!(!sources.is_empty(), "actual B must create encrypted response while A waits");
    assert!(service.sync_gateway.sync(h.approver.user()).await.is_err());
    ensure!(protocol.ack.load(Ordering::SeqCst) > 0, "actual server ACK required");
    ensure!(protocol.fault_hit.load(Ordering::SeqCst) > 0, "later actual download fault required");
    for source in &sources {
        use meta_secret_core::node::db::events::generic_log_event::ObjIdExtractor;
        ensure!(h.p.repo.find_one(source.obj_id()).await?.is_none(),
            "actual gateway must delete exact acknowledged local source");
    }
    protocol.fail_download.store(false, Ordering::SeqCst);
    let prior_acks = protocol.ack.load(Ordering::SeqCst);
    drop(b_owner);
    assert!(a.await.is_err(),
        "waiting A must refuse after B actual ACK/delete and failed download");
    assert_eq!(protocol.ack.load(Ordering::SeqCst), prior_acks,
        "A must never publish a second response after B was acknowledged");
    for source in &sources {
        use meta_secret_core::node::db::events::generic_log_event::ObjIdExtractor;
        ensure!(h.p.repo.find_one(source.obj_id()).await?.is_none(),
            "A must not leave a reminted local response");
    }
    let fresh = service.get_app_state().await?;
    let ApplicationState::Vault(VaultFullInfo::Member(member)) = fresh else {
        bail!("successful canonical read must retain actual member scope")
    };
    let fresh_claim = member.ss_claims.claims.get(&claim.id).context("exact canonical claim")?;
    assert_ne!(fresh_claim.client_status, Some(RecoveryClientStatus::NeedApprove),
        "BLOCKER: acknowledged response source vanished but fresh canonical read permits another Approve");
    Ok::<_, anyhow::Error>(())
    }).await.context("test-only five-second owner-overlap bound elapsed")?
}
