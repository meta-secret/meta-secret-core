use super::*;
use crate::{
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        common::model::user::common::{UserDataMember, UserDataOutsider},
        db::events::{
            object_id::Next,
            vault::{
                liveboard_metadata::RemovalSourceProof,
                vault_log_event::{RemoveMemberEvent, RemoveMemberRequestEvent},
            },
        },
    },
};

#[test]
fn selected_removal_history_rejects_immutable_conflicts() -> Result<()> {
    // Typed metadata only: this does not claim a ServerApp/signature proof.
    let state = FixtureRegistry::empty().state;
    let owner = UserDataMember::from(state.user_creds.client.user());
    let target = UserDataMember::from(state.user_creds.client_b.user());
    let base = ArtifactId::from(VaultDescriptor::from(owner.user_data.vault_name.clone()));
    let uuid = "00000000-0000-4000-8000-000000000001".to_owned();
    let request = RemoveMemberRequestEvent {
        sender: owner.clone(),
        target: target.clone(),
        share_transfers: vec![],
    };
    let receipt = RemovalApplicationReceipt {
        request_id: uuid.clone(),
        base_vault_artifact_id: base.clone(),
        target_vault_artifact_id: base.next(),
        update: RemoveMemberEvent {
            request,
            sender: owner.clone(),
            target: target.clone(),
            new_status: UserMembership::Outsider(UserDataOutsider {
                user_data: target.user_data.clone(),
                status: UserDataOutsiderStatus::Declined,
            }),
            decision: RemoveMemberDecision::Accepted,
            share_transfers: vec![],
        },
    };
    let result = RemovalResultRecord {
        request_id: uuid,
        requester_id: owner.user_data.device.device_id.clone(),
        target_id: target.user_data.device.device_id.clone(),
        responder_id: owner.user_data.device.device_id.clone(),
        outcome: DecisionOutcome::Accepted,
        application_state: RemovalApplicationState::Prepared,
        source: Some(RemovalSourceProof {
            event_id: ArtifactId::from(DeviceLogDescriptor::from(owner.user_data.user_id())),
            signer: owner.user_data.device.device_id.clone(),
            signed_action_hash: "component-only-source-commitment".into(),
        }),
    };
    let mut first = VaultLogObject::create(owner);
    first.0.value.liveboard_removal_applications.push(receipt);
    first.0.value.liveboard_removal_results.push(result);
    let mut last = first.clone();
    last.0.key = last.0.key.next();
    last.0.value.liveboard_removal_applications.clear();
    last.0.value.liveboard_removal_results[0].application_state = RemovalApplicationState::Applied;
    let expected = last.0.value.liveboard_removal_results[0].clone();
    let healthy = vec![first, last];
    for log in &healthy {
        log.0.value.validate_removal_bindings(true)?;
    }
    assert_eq!(applied_at(&healthy, &expected)?, 1);
    let mut outcome = healthy.clone();
    outcome[0].0.value.liveboard_removal_results[0].outcome = DecisionOutcome::Declined;
    let mut source = healthy.clone();
    source[0].0.value.liveboard_removal_results[0].source = None;
    let mut receipt = healthy.clone();
    receipt[0].0.value.liveboard_removal_applications[0]
        .update
        .decision = RemoveMemberDecision::Declined;
    for history in [&outcome, &source, &receipt] {
        for log in history {
            log.0.value.validate_removal_bindings(true)?;
        }
        assert_eq!(history[1], healthy[1]);
    }
    let outcome_rejected = applied_at(&outcome, &expected).is_err();
    let source_rejected = applied_at(&source, &expected).is_err();
    let receipt_rejected = applied_at(&receipt, &expected).is_err();
    assert!(
        outcome_rejected && source_rejected && receipt_rejected,
        "selected history conflicts: outcome={outcome_rejected} source={source_rejected} receipt={receipt_rejected}"
    );
    Ok(())
}
