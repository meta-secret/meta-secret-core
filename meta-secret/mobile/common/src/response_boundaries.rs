//! Recovery response persistence faults and native owner-interval regressions.
#[path = "response_fault_repo.rs"]
mod response_repo;
use super::{fixtures, transport};
use anyhow::{Result, ensure};
use meta_secret_core::node::{
    app::{
        meta_app::{
            client_transfer::MetaClientRequestTransfer,
            meta_client_service::{
                MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
            },
        },
        sync::sync_gateway::SyncGateway,
    },
    common::model::user::common::UserDataMember,
    db::{
        events::{generic_log_event::GenericKvLogEvent, shared_secret_event::SsWorkflowObject},
        objects::{
            persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
        },
        repo::generic_db::*,
    },
};
use response_repo::ResponseRepo;
use std::{
    future::{Future, poll_fn},
    sync::{Arc, atomic::Ordering},
    task::Poll,
};
async fn setup() -> Result<(
    fixtures::Harness,
    Arc<ResponseRepo>,
    MetaClientService<ResponseRepo, transport::Protocol>,
    meta_secret_core::node::common::model::secret::ClaimId,
)> {
    let mut h = fixtures::Harness::new(3).await?;
    let pass = h.seed_secret("controlled-response-boundary").await?;
    let claim = h
        .vault
        .clone()
        .to_vault_member(UserDataMember::from(h.initiator.user()))?
        .create_recovery_claim(pass);
    PersistentSharedSecret::from(h.p.clone())
        .save_ss_log_event(claim.clone())
        .await?;
    let repo = Arc::new(ResponseRepo::default());
    for event in h.p.repo.get_db().await.into_values() {
        repo.inner.save(event).await?;
    }
    let p = Arc::new(PersistentObject::new(repo.clone()));
    let service = MetaClientService {
        data_transfer: Arc::new(MetaClientDataTransfer {
            dt: MetaClientRequestTransfer::new(),
        }),
        sync_gateway: Arc::new(SyncGateway::new(
            "actual-response-boundary".into(),
            p.clone(),
            h.protocol.clone(),
            h.master.clone(),
        )),
        state_provider: Arc::new(MetaClientStateProvider::new()),
        p_obj: p,
        device_data: h.approver.device(),
        master_key: h.master.clone(),
    };
    Ok((h, repo, service, claim.id))
}
async fn count(repo: &ResponseRepo) -> usize {
    repo.inner
        .get_db()
        .await
        .into_values()
        .filter(|event| {
            matches!(
                event,
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_))
            )
        })
        .count()
}
#[tokio::test]
async fn actual_response_error_before_save_differs_from_error_after_commit() -> Result<()> {
    for after in [false, true] {
        let (_h, repo, service, claim) = setup().await?;
        if after {
            repo.fail_after.store(true, Ordering::SeqCst);
        } else {
            repo.fail_before.store(true, Ordering::SeqCst);
        }
        assert!(service.accept_recover(claim).await.is_err());
        assert_eq!(count(&repo).await, usize::from(after));
        assert_eq!(repo.saves.load(Ordering::SeqCst), usize::from(after));
    }
    Ok(())
}
#[tokio::test]
async fn actual_writer_waits_existing_same_client_owner_without_response_side_effects() -> Result<()>
{
    let (_h, repo, service, claim) = setup().await?;
    let owner = service.p_obj.client_operation_guard.lock().await;
    let mut writer = std::pin::pin!(service.accept_recover(claim));
    poll_fn(|cx| {
        assert!(writer.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    ensure!(
        count(&repo).await == 0 && repo.saves.load(Ordering::SeqCst) == 0,
        "actual blocked writer must not create a response"
    );
    drop(owner);
    writer.await?;
    ensure!(
        count(&repo).await > 0 && repo.saves.load(Ordering::SeqCst) > 0,
        "same actual writer must execute once owner releases"
    );
    Ok(())
}
// Candidate-only APIs below: execute only after owning implementation adoption.
// A1 public JSON remains the semantic RED; missing API is never called semantic RED.
#[derive(Default)]
struct CanonicalGate {
    armed: std::sync::atomic::AtomicBool,
    reached: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
#[derive(Debug)]
struct ControlledOffline;
impl std::fmt::Display for ControlledOffline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("controlled actual pre-sync outage")
    }
}
impl std::error::Error for ControlledOffline {}
struct ActualServer {
    server: Arc<
        meta_server_node::server::server_app::ServerApp<
            meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo,
        >,
    >,
    gate: Option<Arc<CanonicalGate>>,
    offline: std::sync::atomic::AtomicBool,
    fail_post_ack: std::sync::atomic::AtomicBool,
    ack: std::sync::atomic::AtomicUsize,
    download_fault_hits: std::sync::atomic::AtomicUsize,
}
impl meta_secret_core::node::app::sync::sync_protocol::SyncProtocol for ActualServer {
    async fn send(
        &self,
        request: meta_secret_core::node::api::SyncRequest,
    ) -> Result<meta_secret_core::node::api::DataSyncResponse> {
        use meta_secret_core::node::api::{ReadSyncRequest, SyncRequest};
        if let Some(gate) = &self.gate
            && matches!(&request, SyncRequest::Read(read)
                if matches!(read.as_ref(), ReadSyncRequest::SsRequest(_)))
            && gate.armed.swap(false, Ordering::SeqCst)
        {
            gate.reached.notify_one();
            gate.release.notified().await;
        }
        if self.offline.load(Ordering::SeqCst) {
            return Err(ControlledOffline.into());
        }
        if self.fail_post_ack.load(Ordering::SeqCst)
            && self.ack.load(Ordering::SeqCst) > 0
            && matches!(&request, SyncRequest::Read(read)
                if matches!(read.as_ref(), ReadSyncRequest::SsRequest(_)))
        {
            self.download_fault_hits.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("controlled actual post-response download outage");
        }
        use meta_secret_core::node::api::{SignedActionPayload, WriteSyncRequest};
        let response_write = matches!(&request, SyncRequest::Write(write)
            if matches!(write.as_ref(), WriteSyncRequest::Event(action)
                if matches!(&action.payload, SignedActionPayload::Event(
                    GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_))))));
        let response = self.server.handle_client_request(request).await?;
        if response_write {
            response.ensure_success()?;
            self.ack.fetch_add(1, Ordering::SeqCst);
        }
        Ok(response)
    }
}
async fn checked_setup(
    gate: Option<Arc<CanonicalGate>>,
) -> Result<(
    fixtures::Harness,
    Arc<ResponseRepo>,
    MetaClientService<ResponseRepo, ActualServer>,
    meta_secret_core::node::common::model::secret::ClaimId,
)> {
    use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
    let (h, repo, original, claim) = setup().await?;
    let server_repo = Arc::new(InMemKvLogEventRepo::default());
    for event in repo.inner.get_db().await.into_values() {
        if !matches!(
            event,
            GenericKvLogEvent::UserCreds(_)
                | GenericKvLogEvent::DeviceCreds(_)
                | GenericKvLogEvent::LocalRemovalEnvelope(_)
        ) {
            server_repo.save(event).await?;
        }
    }
    let server = Arc::new(meta_server_node::server::server_app::ServerApp::new(
        server_repo,
        h.master.clone(),
    )?);
    server.init().await?;
    let protocol = Arc::new(ActualServer {
        server,
        gate,
        offline: false.into(),
        fail_post_ack: false.into(),
        ack: 0.into(),
        download_fault_hits: 0.into(),
    });
    let p = original.p_obj.clone();
    let service = MetaClientService {
        data_transfer: Arc::new(MetaClientDataTransfer {
            dt: MetaClientRequestTransfer::new(),
        }),
        sync_gateway: Arc::new(SyncGateway::new(
            "actual-checked-entry".into(),
            p.clone(),
            protocol,
            h.master.clone(),
        )),
        state_provider: Arc::new(MetaClientStateProvider::new()),
        p_obj: p,
        device_data: h.approver.device(),
        master_key: h.master.clone(),
    };
    Ok((h, repo, service, claim))
}
#[tokio::test]
async fn checked_actual_before_and_after_save_failures_remain_errors_with_exact_commit_truth()
-> Result<()> {
    for after in [false, true] {
        let (h, repo, service, claim) = checked_setup(None).await?;
        if after {
            repo.fail_after.store(true, Ordering::SeqCst);
        } else {
            repo.fail_before.store(true, Ordering::SeqCst);
        }
        let result = service
            .accept_recover_revalidated(claim, h.approver.user())
            .await;
        assert!(
            result.is_err(),
            "actual checked executor fault must remain failure"
        );
        ensure!(
            !repo.fail_before.load(Ordering::SeqCst) && !repo.fail_after.load(Ordering::SeqCst),
            "armed response fault must actually be consumed"
        );
        assert_eq!(
            repo.saves.load(Ordering::SeqCst),
            usize::from(after),
            "fault must reach actual response-save boundary, not earlier validation"
        );
        assert_eq!(count(&repo).await, usize::from(after));
    }
    Ok(())
}
#[tokio::test]
async fn checked_actual_entry_waits_same_owner_then_observes_prior_real_response() -> Result<()> {
    let (h, repo, checked, claim) = checked_setup(None).await?;
    let owner = checked.p_obj.client_operation_guard.lock().await;
    let mut future =
        std::pin::pin!(checked.accept_recover_revalidated(claim.clone(), h.approver.user()));
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    assert_eq!(repo.saves.load(Ordering::SeqCst), 0);
    // This owner is the deterministic B-before-A schedule. Write through the
    // actual orchestrator while owning the same client guard, never copied logic.
    meta_secret_core::node::app::orchestrator::MetaOrchestrator {
        p_obj: checked.p_obj.clone(),
        user_creds: h.approver.clone(),
    }
    .accept_recover(claim.clone())
    .await?;
    let before = repo.saves.load(Ordering::SeqCst);
    ensure!(before > 0, "actual B response required");
    drop(owner);
    assert!(
        future.await.is_err(),
        "A must refuse prior B decision after fresh canonical read"
    );
    assert_eq!(
        repo.saves.load(Ordering::SeqCst),
        before,
        "checked A must not encrypt/save a second response"
    );
    Ok(())
}
#[tokio::test]
async fn checked_actual_identity_mismatch_never_dispatches_response() -> Result<()> {
    let (h, repo, service, claim) = checked_setup(None).await?;
    assert!(
        service
            .accept_recover_revalidated(claim, h.initiator.user())
            .await
            .is_err()
    );
    assert_eq!(repo.saves.load(Ordering::SeqCst), 0);
    assert_eq!(count(&repo).await, 0);
    Ok(())
}

#[tokio::test]
async fn checked_actual_owner_blocks_writer_during_fresh_canonical_read_through_entry() -> Result<()>
{
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let gate = Arc::new(CanonicalGate::default());
        gate.armed.store(true, Ordering::SeqCst);
        let (h, repo, service, claim) = checked_setup(Some(gate.clone())).await?;
        let a = service.accept_recover_revalidated(claim.clone(), h.approver.user());
        let observer = async {
            gate.reached.notified().await;
            // A is inside actual canonical sync while already holding the owner.
            let mut b = std::pin::pin!(service.accept_recover(claim.clone()));
            poll_fn(|cx| {
                assert!(b.as_mut().poll(cx).is_pending());
                Poll::Ready(())
            })
            .await;
            assert_eq!(repo.saves.load(Ordering::SeqCst), 0);
            // B stays unpolled and is cancelled at this observer's lexical scope end;
            // it never crossed owner admission or submitted a response.
            gate.release.notify_one();
        };
        let (result, ()) = tokio::join!(a, observer);
        result?;
        ensure!(
            repo.saves.load(Ordering::SeqCst) > 0,
            "actual A must create response after read"
        );
        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|_| anyhow::anyhow!("test-only five-second canonical gate bound elapsed"))?
}

#[tokio::test]
async fn actual_fixed_slot_read_errors_and_malformed_owned_source_veto_attempt_evidence()
-> Result<()> {
    use meta_secret_core::node::db::{
        descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
        events::{generic_log_event::ObjIdExtractor, kv_log_event::KvKey},
    };
    for mode in [
        "missing-read-error",
        "existing-read-error",
        "malformed-owned-source",
    ] {
        let (h, repo, service, claim_id) = checked_setup(None).await?;
        let log = PersistentSharedSecret::from(service.p_obj.clone())
            .get_ss_log_obj(h.approver.vault_name.clone())
            .await?;
        let claim = log
            .claims
            .get(&claim_id)
            .expect("actual exact seeded claim");
        let own = claim
            .recovery_db_ids()
            .into_iter()
            .find(|id| id.distribution_id.receiver == *h.approver.device_id())
            .expect("actual own response slot");
        let key = KvKey::from(SsWorkflowDescriptor::Recovery(own)).obj_id;
        if mode != "missing-read-error" {
            meta_secret_core::node::app::orchestrator::MetaOrchestrator {
                p_obj: service.p_obj.clone(),
                user_creds: h.approver.clone(),
            }
            .accept_recover(claim_id.clone())
            .await?;
        }
        if mode == "malformed-owned-source" {
            let GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(mut event)) = repo
                .inner
                .find_one(key.clone())
                .await?
                .expect("actual encrypted response")
            else {
                anyhow::bail!("actual response type required")
            };
            event.value.claim_id.id = meta_secret_core::node::common::model::secret::ClaimId::from(
                meta_secret_core::crypto::utils::Id48bit::generate(),
            );
            repo.inner
                .replace(SsWorkflowObject::Recovery(event))
                .await?;
        } else {
            *repo.target_read_error.lock().unwrap() = Some(key.clone());
        }
        let before = repo.inner.find_one(key).await?;
        service
            .sync_gateway
            .sync
            .offline
            .store(true, Ordering::SeqCst);
        let original = service
            .sync_gateway
            .sync(h.approver.user())
            .await
            .expect_err("actual controlled pre-sync outage");
        let message = original.to_string();
        let result = service
            .recover_approve_presync_failure(claim_id, h.approver.user(), original)
            .await;
        ensure!(
            result.not_submitted.is_none(),
            "unknown physical source state must veto evidence"
        );
        let preserved = result.result.expect_err("original failure");
        ensure!(
            preserved.is::<ControlledOffline>() && preserved.to_string() == message,
            "audit failure must preserve original operation error type and message"
        );
        if mode != "malformed-owned-source" {
            ensure!(
                repo.read_error_hits.load(Ordering::SeqCst) > 0,
                "actual fixed-slot read fault must be reached even for absent key"
            );
        }
        if let Some(event) = before {
            ensure!(
                repo.inner.find_one(event.obj_id()).await? == Some(event),
                "source audit must never rewrite/delete malformed or uncertain source"
            );
        }
    }
    Ok(())
}
#[tokio::test]
async fn actual_upper_typed_attempt_has_no_metadata_after_dispatch_or_postsync_failure()
-> Result<()> {
    for mode in ["before-save", "after-save", "post-sync", "success"] {
        let (h, repo, service, claim) = checked_setup(None).await?;
        match mode {
            "before-save" => repo.fail_before.store(true, Ordering::SeqCst),
            "after-save" => repo.fail_after.store(true, Ordering::SeqCst),
            "post-sync" => service
                .sync_gateway
                .sync
                .fail_post_ack
                .store(true, Ordering::SeqCst),
            "success" => {}
            _ => unreachable!(),
        }
        let service = Arc::new(service);
        let manager = crate::app_manager::ApplicationManager::new(
            service.sync_gateway.sync.clone(),
            service.sync_gateway.clone(),
            service.clone(),
            h.master.clone(),
        );
        // Actual FIFO service owns its normal GetState publication, not a mirrored state port.
        let attempt = tokio::select! {
            result = manager.accept_recover_mobile_attempt(claim) => result,
            _ = service.run() => anyhow::bail!("actual FIFO service ended before result"),
        };
        ensure!(
            attempt.not_submitted.is_none(),
            "after-dispatch outcome must never gain optional proof"
        );
        if mode == "success" {
            attempt.result?;
        } else {
            ensure!(
                attempt.result.is_err(),
                "actual armed failure must remain error"
            );
        }
        ensure!(
            !repo.fail_before.load(Ordering::SeqCst) && !repo.fail_after.load(Ordering::SeqCst),
            "actual save faults must be consumed before result"
        );
        assert_eq!(
            repo.saves.load(Ordering::SeqCst),
            usize::from(mode != "before-save")
        );
        if mode == "post-sync" {
            ensure!(
                service.sync_gateway.sync.ack.load(Ordering::SeqCst) > 0
                    && service
                        .sync_gateway
                        .sync
                        .download_fault_hits
                        .load(Ordering::SeqCst)
                        > 0,
                "real response ACK and later download fault required"
            );
        }
    }
    Ok(())
}
#[tokio::test]
async fn actual_presync_wrong_subject_and_claim_preserve_original_error_without_evidence()
-> Result<()> {
    for wrong_actor in [false, true] {
        let (h, repo, service, mut claim) = checked_setup(None).await?;
        service
            .sync_gateway
            .sync
            .offline
            .store(true, Ordering::SeqCst);
        let original = service
            .sync_gateway
            .sync(h.approver.user())
            .await
            .expect_err("actual controlled pre-sync outage");
        let message = original.to_string();
        let expected = if wrong_actor {
            h.initiator.user()
        } else {
            h.approver.user()
        };
        if !wrong_actor {
            claim = meta_secret_core::node::common::model::secret::ClaimId::from(
                meta_secret_core::crypto::utils::Id48bit::generate(),
            );
        }
        let result = service
            .recover_approve_presync_failure(claim, expected, original)
            .await;
        ensure!(
            result.not_submitted.is_none(),
            "unknown claim/foreign subject cannot gain proof"
        );
        let preserved = result.result.expect_err("original failure");
        ensure!(
            preserved.is::<ControlledOffline>() && preserved.to_string() == message,
            "identity audit must preserve original pre-sync error type and message"
        );
        assert_eq!(repo.saves.load(Ordering::SeqCst), 0);
    }
    Ok(())
}
