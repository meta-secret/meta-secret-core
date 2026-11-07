//! Native recovery-approval regression controls using real application APIs.
#[path = "recovery_approve_ack_tests.rs"]
mod ack_delete;
#[path = "response_boundaries.rs"]
mod response_boundaries;

use super::{fault_repo, seed_fixture as fixtures, transport};
use crate::app_manager::ApplicationManager;
use anyhow::{Result, bail};
use meta_secret_core::node::{
    api::{DataSyncResponse, SyncRequest},
    app::{
        meta_app::{
            client_transfer::MetaClientRequestTransfer,
            meta_client_service::{
                MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
            },
        },
        sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol},
    },
    common::model::user::common::UserDataMember,
    db::{
        events::{generic_log_event::GenericKvLogEvent, shared_secret_event::SsWorkflowObject},
        objects::persistent_shared_secret::PersistentSharedSecret,
    },
};
use std::sync::Arc;
struct Offline;
impl SyncProtocol for Offline {
    async fn send(&self, _: SyncRequest) -> Result<DataSyncResponse> {
        bail!("controlled pre-sync failure")
    }
}
fn manager(h: &fixtures::Harness) -> ApplicationManager<fault_repo::Repo, Offline> {
    let server = Arc::new(Offline);
    let gateway = Arc::new(SyncGateway::new(
        "recovery-attempt-baseline".into(),
        h.p.clone(),
        server.clone(),
        h.master.clone(),
    ));
    let service = Arc::new(MetaClientService {
        data_transfer: Arc::new(MetaClientDataTransfer {
            dt: MetaClientRequestTransfer::new(),
        }),
        sync_gateway: gateway.clone(),
        state_provider: Arc::new(MetaClientStateProvider::new()),
        p_obj: h.p.clone(),
        device_data: h.approver.device(),
        master_key: h.master.clone(),
    });
    ApplicationManager::new(server, gateway, service, h.master.clone())
}
async fn response_sources(h: &fixtures::Harness) -> usize {
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
        .count()
}
#[tokio::test]
async fn actual_pre_sync_failure_has_no_new_response_source() -> Result<()> {
    let mut h = fixtures::Harness::new(3).await?;
    let pass = h.seed_secret("controlled-recovery-attempt").await?;
    let claim = h
        .vault
        .clone()
        .to_vault_member(UserDataMember::from(h.initiator.user()))?
        .create_recovery_claim(pass);
    PersistentSharedSecret::from(h.p.clone())
        .save_ss_log_event(claim.clone())
        .await?;
    assert_eq!(response_sources(&h).await, 0);
    assert!(manager(&h).accept_recover_mobile(claim.id).await.is_err());
    assert_eq!(response_sources(&h).await, 0);
    Ok(())
}
#[tokio::test]
async fn actual_pre_sync_failure_does_not_prove_absence_of_prior_response() -> Result<()> {
    let mut h = fixtures::Harness::new(3).await?;
    let pass = h.seed_secret("controlled-queued-recovery").await?;
    let claim = h
        .vault
        .clone()
        .to_vault_member(UserDataMember::from(h.initiator.user()))?
        .create_recovery_claim(pass);
    PersistentSharedSecret::from(h.p.clone())
        .save_ss_log_event(claim.clone())
        .await?;
    h.service().accept_recover(claim.id.clone()).await?;
    let before = response_sources(&h).await;
    assert!(
        before > 0,
        "actual prior accept must create real encrypted response"
    );
    assert!(manager(&h).accept_recover_mobile(claim.id).await.is_err());
    assert_eq!(response_sources(&h).await, before);
    Ok(())
}
