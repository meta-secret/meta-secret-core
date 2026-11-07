//! Independent checked-port regression against the actual signed server boundary.
#[path = "../../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "../../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "liveboard_claim_orphan/repo.rs"]
mod repo;
#[path = "../../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;

use anyhow::{Context, Result};
use fixtures::Harness;
use meta_secret_core::node::{
    api::*,
    app::{
        meta_app::meta_client_service::MetaClientService,
        sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol},
    },
    common::model::liveboard::LiveboardOperationPhase,
    db::{
        actions::recover::RecoveryAction,
        events::{generic_log_event::*, object_id::Next},
        objects::{
            persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
        },
        repo::generic_db::*,
    },
    security::sign_event,
};
use meta_server_node::server::server_app::ServerApp;
use std::sync::Arc;

struct ActualServer(Arc<ServerApp<repo::Repo>>);
impl SyncProtocol for ActualServer {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        self.0.handle_client_request(request).await
    }
}

#[tokio::test]
async fn processed_raw_claim_without_canonical_echo_is_not_returned_as_healthy_queued_recovery()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("server-raw-claim-orphan").await?;
    let server_repo = Arc::new(repo::Repo::default());
    // Copy only real canonical records; credentials and client queue remain local.
    for event in h.p.repo.get_db().await.into_values() {
        if matches!(
            event,
            GenericKvLogEvent::Vault(_)
                | GenericKvLogEvent::VaultLog(_)
                | GenericKvLogEvent::VaultStatus(_)
                | GenericKvLogEvent::SsLog(_)
        ) {
            server_repo.save(event).await?;
        }
    }
    let server = ServerApp::new(server_repo.clone(), h.master.clone())?;
    server.init().await?;
    let ss = PersistentSharedSecret::from(h.p.clone());
    let original = RecoveryAction::from(h.p.clone())
        .recovery_request_checked(h.approver.clone(), pass.clone())
        .await?;
    let event = ss
        .find_ss_device_log_tail_event(h.approver.device_id().clone())
        .await?
        .context("actual durably queued Recovery")?;
    let actual_claim = event.clone().to_distribution_request();
    assert_eq!(
        original.claim_id.as_deref(),
        Some(actual_claim.id.wasm_id_str().as_str())
    );
    // The real server must expect this exact stream sequence, including any
    // earlier genuine Split records produced by the fixture's distributor.
    for previous in h.p.get_object_events_from_beginning(
        meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsDeviceLogDescriptor::from(
            h.approver.device_id().clone(),
        ),
    ).await? {
        if previous.obj_id() != event.obj_id() { server_repo.save(previous).await?; }
    }
    let signed = sign_event(
        event.clone().to_generic(),
        h.approver.device_id().clone(),
        &h.approver.device_creds.key_manager()?.dsa,
    )?;
    // Historical pre-upgrade compatibility fixture. B2 deliberately prevents
    // new raw-first orphans; seed that documented old DB state directly from
    // this ACTUAL original signed local source, with no inferred intake proof.
    // The previous genuine current-server fault reproduction/hash/log is kept
    // in the run artifacts; no test-only legacy business handler is introduced.
    signed.verify(&h.approver.device().keys.dsa_pk)?;
    assert!(signed.event()? == &event.clone().to_generic());
    server_repo.save(signed.event()?.clone()).await?;
    assert!(
        server_repo.find_one(event.obj_id()).await?.is_some(),
        "historical exact raw source must already be durable"
    );
    let server_ss =
        PersistentSharedSecret::from(Arc::new(PersistentObject::new(server_repo.clone())));
    assert!(
        !server_ss
            .get_ss_log_obj(h.vault.vault_name.clone())
            .await?
            .claims
            .contains_key(&actual_claim.id)
    );
    let reopened = Arc::new(ServerApp::new(server_repo.clone(), h.master.clone())?);
    let tail = reopened
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
            ServerTailRequest {
                producer_reference: None,
                sender: h.approver.user(),
            },
        ))))
        .await?
        .to_server_tail()?;
    assert_eq!(
        tail.ss_device_log_tail,
        Some(event.obj_id().next()),
        "actual server tail skips the orphan on ordinary flush"
    );
    let original_service = h.service();
    let service = MetaClientService {
        data_transfer: original_service.data_transfer,
        sync_gateway: Arc::new(SyncGateway::new(
            "actual-orphan-server".into(),
            h.p.clone(),
            Arc::new(ActualServer(reopened)),
            h.master.clone(),
        )),
        state_provider: original_service.state_provider,
        p_obj: h.p.clone(),
        device_data: h.approver.device(),
        master_key: h.master.clone(),
    };
    let result = service.recover_checked(pass).await;
    // This proofless historical source must remain the SAME-ID Unknown. B2
    // grants continuation only to a future durably prepared initial request.
    let canonical = server_ss.get_ss_log_obj(h.vault.vault_name.clone()).await?;
    assert!(
        !canonical.claims.contains_key(&actual_claim.id),
        "historical proofless orphan must not be healed by retrospective inference"
    );
    assert!(canonical.pending_recovery_request_intake.is_none());
    if !canonical.claims.contains_key(&actual_claim.id)
        && let Ok(receipt) = &result
    {
        assert_eq!(receipt.claim_id, original.claim_id);
        assert_eq!(
            receipt.phase,
            LiveboardOperationPhase::Unknown,
            "processed raw Recovery lacking canonical state must fail closed or report Unknown, not healthy Queued"
        );
    }
    let local_tail = ss
        .find_ss_device_log_tail_event(h.approver.device_id().clone())
        .await?
        .context("unchanged actual local queue")?;
    assert!(
        local_tail == event,
        "uncertainty must not allocate another Claim ID"
    );
    Ok(())
}
