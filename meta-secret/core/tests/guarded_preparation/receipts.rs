use super::fixtures::REQUEST;
use anyhow::Result;
use meta_secret_core::{
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        api::RemovalDecisionReceipt,
        common::model::liveboard::LiveboardOperationPhase,
        db::{
            descriptors::vault_descriptor::DeviceLogDescriptor,
            events::object_id::{ArtifactId, Next},
        },
    },
};

#[test]
fn guarded_preparation_old_receipt_json_defaults_to_queued_without_changing_actual_identity()
-> Result<()> {
    // Typed codec fixture only: no checked producer, transport or Applied claim.
    let actor = FixtureRegistry::empty().state.user_creds.client_b;
    let current = RemovalDecisionReceipt {
        request_id: REQUEST.into(),
        event_id: ArtifactId::from(DeviceLogDescriptor::from(actor.user_id())).next(),
        phase: LiveboardOperationPhase::Queued,
        error_code: None,
    };
    let mut wire = serde_json::to_value(&current)?;
    wire.as_object_mut().unwrap().remove("phase");
    wire.as_object_mut().unwrap().remove("errorCode");
    let old: RemovalDecisionReceipt = serde_json::from_value(wire)?;
    assert_eq!(old.phase, LiveboardOperationPhase::Queued);
    assert_eq!(old.error_code, None);
    assert_eq!(old.event_id, current.event_id);
    assert_eq!(old.request_id, current.request_id);
    Ok(())
}
