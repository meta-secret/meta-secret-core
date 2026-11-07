//! Canonical operation identities and existing Core eligibility rules.
use crate::node::common::model::{
    liveboard::{LiveboardOperationKind as Kind, LiveboardOperationState as Op},
    secret::{
        RecoveryClientStatus as Status, SecretDistributionType, SsDistributionStatus, SsLogData,
    },
    vault::vault::VaultMember,
};
use crate::node::db::events::vault::{
    liveboard_metadata::{DecisionOutcome, RemovalApplicationState},
    vault_log_event::{VaultActionEvents, VaultActionRequestEvent as Request},
};

pub(super) fn operations(
    member: &VaultMember,
    events: &VaultActionEvents,
    claims: &SsLogData,
) -> Vec<Op> {
    let vault = &member.vault;
    let current = member.member.user_data.device.device_id.to_string();
    let mut result = vec![];
    for request in &events.requests {
        match request {
            Request::JoinCluster(join) if !join.request_id.is_empty() => {
                let mut op = Op::pending(
                    Kind::Join,
                    join.request_id.clone(),
                    join.candidate.device.device_id.to_string(),
                );
                op.eligibility(
                    &current,
                    vault
                        .members()
                        .iter()
                        .map(|m| m.user_data.device.device_id.to_string())
                        .collect(),
                );
                op.can_approve &= vault
                    .ensure_can_add_member(
                        &crate::node::common::model::user::common::UserDataMember::from(
                            join.candidate.clone(),
                        ),
                    )
                    .is_ok();
                result.push(op);
            }
            Request::RemoveMember(remove) => {
                let bindings: Vec<_> = events
                    .liveboard_removal_pending
                    .iter()
                    .filter(|b| b.request == *remove)
                    .collect();
                let [binding] = bindings.as_slice() else {
                    continue;
                };
                if vault.ensure_can_remove_member(remove, None).is_err() {
                    continue;
                }
                let mut op = Op::pending(
                    Kind::RemoveDevice,
                    binding.request_id.clone(),
                    remove.sender.user_data.device.device_id.to_string(),
                );
                op.target_device_id = Some(remove.target.user_data.device.device_id.to_string());
                op.eligibility(
                    &current,
                    vault
                        .members()
                        .iter()
                        .filter(|m| vault.ensure_can_remove_member(remove, Some(m)).is_ok())
                        .map(|m| m.user_data.device.device_id.to_string())
                        .collect(),
                );
                result.push(op);
            }
            Request::DeleteSecret(delete) => {
                if vault.deletion_results.contains_key(&delete.request_id)
                    || vault.ensure_can_delete_secret(delete, None).is_err()
                {
                    continue;
                }
                let mut op = Op::pending(
                    Kind::DeleteSecret,
                    delete.request_id.clone(),
                    delete.sender.user_data.device.device_id.to_string(),
                );
                op.secret_id = Some(delete.pass_id.id_str());
                op.eligibility(
                    &current,
                    vault
                        .members()
                        .iter()
                        .filter(|m| vault.ensure_can_delete_secret(delete, Some(m)).is_ok())
                        .map(|m| m.user_data.device.device_id.to_string())
                        .collect(),
                );
                result.push(op);
            }
            _ => {}
        }
    }
    for claim in claims
        .claims
        .values()
        .filter(|c| c.distribution_type == SecretDistributionType::Recover)
    {
        let mut op = Op::pending(
            Kind::Recover,
            claim.id.wasm_id_str(),
            claim.sender.to_string(),
        );
        op.secret_id = Some(claim.dist_claim_id.pass_id.id_str());
        op.client_status = match claim.client_status {
            Some(Status::Pending) => "pending",
            Some(Status::NeedApprove) => "needApprove",
            Some(Status::Accepted) => "accepted",
            Some(Status::Declined) => "declined",
            Some(Status::Done) => "done",
            None => "unknown",
        }
        .into();
        let pending = claim
            .status
            .statuses
            .values()
            .all(|s| *s == SsDistributionStatus::Pending);
        if pending && vault.is_member(&claim.sender) {
            op.eligibility(
                &current,
                claim
                    .receivers
                    .iter()
                    .filter(|id| **id != claim.sender && vault.is_member(id))
                    .map(ToString::to_string)
                    .collect(),
            );
        }
        if let Some(decision) = claims.liveboard_recovery_results.get(&claim.id) {
            op.outcome = Some(outcome(decision.outcome));
            op.responder_id = Some(decision.responder_id.to_string());
            op.resolution_source = "verified".into();
        }
        if matches!(
            claim.client_status,
            Some(Status::Accepted | Status::Declined | Status::Done)
        ) {
            op.can_approve = false;
            op.can_decline = false;
        }
        result.push(op);
    }
    for settled in &events.liveboard_removal_results {
        let mut op = Op::pending(
            Kind::RemoveDevice,
            settled.request_id.clone(),
            settled.requester_id.to_string(),
        );
        op.target_device_id = Some(settled.target_id.to_string());
        op.responder_id = Some(settled.responder_id.to_string());
        op.outcome = Some(outcome(settled.outcome));
        op.resolution_source = "verified".into();
        op.client_status = if settled.application_state == RemovalApplicationState::Applied {
            "done"
        } else {
            "synchronizing"
        }
        .into();
        op.readiness = op.client_status.clone();
        result.push(op);
    }
    let mut deletions = vault.deletion_results.clone();
    for d in &events.deletion_results {
        deletions
            .entry(d.request.request_id.clone())
            .or_insert_with(|| d.clone());
    }
    for decision in deletions.values() {
        let mut op = Op::pending(
            Kind::DeleteSecret,
            decision.request.request_id.clone(),
            decision
                .request
                .sender
                .user_data
                .device
                .device_id
                .to_string(),
        );
        op.secret_id = Some(decision.request.pass_id.id_str());
        let canonical = vault
            .deletion_results
            .get(&decision.request.request_id)
            .is_some_and(|actual| actual == decision);
        if canonical {
            op.responder_id = Some(decision.sender.user_data.device.device_id.to_string());
            op.resolution_source = "canonical".into();
        }
        op.outcome = Some(
            if decision.accepted {
                "accepted"
            } else {
                "declined"
            }
            .into(),
        );
        op.client_status = if decision.accepted {
            "done"
        } else {
            "declined"
        }
        .into();
        op.readiness = "settled".into();
        result.push(op);
    }
    result.sort_by(|a, b| a.request_id.cmp(&b.request_id));
    result
}
fn outcome(value: DecisionOutcome) -> String {
    match value {
        DecisionOutcome::Accepted => "accepted",
        DecisionOutcome::Declined => "declined",
    }
    .into()
}
