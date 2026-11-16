use super::{fault_repo::Fault, fixtures::*};
use anyhow::Result;
use meta_secret_core::node::api::CanonicalDecisionInput;
use meta_secret_core::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
use meta_secret_core::node::db::events::vault::device_log_event::DeviceLogObject;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn guarded_preparation_unsupported_control_has_no_queued_body_preparation_or_transport_write()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    h.seed_secret("guarded-preparation-unsupported").await?;
    h.seed_removal().await?;
    h.protocol.capability.store(0, Ordering::SeqCst);
    h.p.repo.arm(Fault::BeforeDistribution);
    let before = h.p.repo.get_db().await;
    assert!(
        h.gateway()
            .decide_remove_member(CanonicalDecisionInput {
                request_id: REQUEST.into(),
                accepted: true,
            })
            .await
            .is_err()
    );
    assert!(h.p.repo.get_db().await == before);
    assert_eq!(h.p.repo.hits.load(Ordering::SeqCst), 0);
    let events: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(h.approver.user_id()))
            .await?;
    assert!(events.is_empty());
    assert!(h.protocol.writes.lock().unwrap().is_empty());
    Ok(())
}
