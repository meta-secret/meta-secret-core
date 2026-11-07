//! Canonical server sidecars. These are never fields of a signed legacy request.
use super::vault_log_event::{
    RemoveMemberEvent, RemoveMemberRequestEvent, VaultActionEvents, VaultActionRequestEvent,
};
use crate::node::common::model::secret::{SecretDistributionType, SsDistributionStatus, SsLogData};
use crate::node::common::model::{device::common::DeviceId, secret::ClaimId};
use crate::node::db::events::object_id::ArtifactId;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRemovalBinding {
    pub request_id: String,
    pub request: RemoveMemberRequestEvent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcome {
    Accepted,
    Declined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalApplicationState {
    Prepared,
    Applied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalResultRecord {
    pub request_id: String,
    pub requester_id: DeviceId,
    pub target_id: DeviceId,
    pub responder_id: DeviceId,
    pub outcome: DecisionOutcome,
    pub application_state: RemovalApplicationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<RemovalSourceProof>,
}

/// Digest of the complete already-verified signed envelope, never a reusable
/// authorization token. Allows only that source item to finish its raw tail.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalSourceProof {
    pub event_id: ArtifactId,
    pub signer: DeviceId,
    pub signed_action_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalApplicationReceipt {
    pub request_id: String,
    pub base_vault_artifact_id: ArtifactId,
    pub target_vault_artifact_id: ArtifactId,
    pub update: RemoveMemberEvent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryDecisionRecord {
    pub claim_id: ClaimId,
    pub responder_id: DeviceId,
    pub outcome: DecisionOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::node::db::repo::recovery_response_store::RecoveryResponseSourceProof>,
}

impl SsLogData {
    /// The server calls this ONLY after verifying the signed workflow actor.
    /// Metadata eligibility is narrower than legacy workflow acceptance: absent
    /// proof of a first pending receiver, keep history explicitly unknown.
    pub fn record_verified_recovery_decision(
        &mut self,
        claim_id: &ClaimId,
        responder_id: &DeviceId,
        outcome: DecisionOutcome,
    ) {
        let Some(claim) = self.claims.get(claim_id) else {
            return;
        };
        if claim.distribution_type != SecretDistributionType::Recover
            || !claim.receivers.contains(responder_id)
            || claim.status.get(responder_id) != Some(&SsDistributionStatus::Pending)
            || claim
                .status
                .statuses
                .values()
                .any(|s| *s != SsDistributionStatus::Pending)
        {
            return;
        }
        self.liveboard_recovery_results
            .entry(claim_id.clone())
            .or_insert_with(|| RecoveryDecisionRecord {
                claim_id: claim_id.clone(),
                responder_id: responder_id.clone(),
                outcome,
                source: None,
            });
    }
}

impl VaultActionEvents {
    /// Missing historical bindings may be initialized; existing inconsistent
    /// bindings are corruption and must never be repaired by rotating identity.
    pub fn initialize_removal_bindings(&mut self) -> Result<bool> {
        self.validate_removal_bindings(false)?;
        let mut changed = false;
        for event in &self.requests {
            if let VaultActionRequestEvent::RemoveMember(request) = event
                && !self
                    .liveboard_removal_pending
                    .iter()
                    .any(|b| b.request == *request)
            {
                self.liveboard_removal_pending.push(PendingRemovalBinding {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    request: request.clone(),
                });
                changed = true;
            }
        }
        self.validate_removal_bindings(true)?;
        Ok(changed)
    }

    pub fn validate_removal_bindings(&self, require_complete: bool) -> Result<()> {
        let mut ids = HashSet::new();
        let mut requests = HashSet::new();
        for binding in &self.liveboard_removal_pending {
            ensure!(
                uuid::Uuid::parse_str(&binding.request_id).is_ok(),
                "invalid removal binding UUID"
            );
            ensure!(
                ids.insert(&binding.request_id) && requests.insert(&binding.request),
                "duplicate removal binding"
            );
            ensure!(
                self.requests
                    .contains(&VaultActionRequestEvent::RemoveMember(
                        binding.request.clone()
                    )),
                "orphan removal binding"
            );
        }
        for result in &self.liveboard_removal_results {
            ensure!(
                uuid::Uuid::parse_str(&result.request_id).is_ok() && ids.insert(&result.request_id),
                "conflicting removal result UUID"
            );
            let receipts: Vec<_> = self
                .liveboard_removal_applications
                .iter()
                .filter(|r| r.request_id == result.request_id)
                .collect();
            ensure!(
                receipts.len()
                    == usize::from(result.application_state == RemovalApplicationState::Prepared),
                "invalid removal application receipt count"
            );
            if let Some(receipt) = receipts.first() {
                ensure!(
                    receipt.update.request.sender.user_data.device.device_id == result.requester_id
                        && receipt.update.target.user_data.device.device_id == result.target_id
                        && receipt.update.sender.user_data.device.device_id == result.responder_id,
                    "removal receipt actor mismatch"
                );
            }
        }
        ensure!(
            self.liveboard_removal_applications.iter().all(|r| self
                .liveboard_removal_results
                .iter()
                .any(|result| result.request_id == r.request_id)),
            "orphan removal application receipt"
        );
        if require_complete {
            ensure!(self.requests.iter().all(|r| !matches!(r, VaultActionRequestEvent::RemoveMember(r) if !requests.contains(r))), "missing removal binding");
        }
        Ok(())
    }
}
