use meta_secret_core::node::common::model::IdString;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::events::object_id::ArtifactId;
pub use meta_secret_core::node::db::events::vault::vault_log_event::JoinResultOutcome;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateInvalidationScope {
    Vault,
    Devices,
    SsClaims,
    All,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateInvalidation {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub vault_name: VaultName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<StateInvalidationScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_result: Option<JoinResultOutcome>,
    /// Present only on the local E2E server when a recovery decision has been
    /// persisted. It contains identifiers and outcome only, never Secret data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub e2e_signal: Option<E2eSignal>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct E2eSignal {
    #[serde(rename = "type")]
    pub signal_type: &'static str,
    pub claim_id: String,
    pub outcome: RecoveryDecisionOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDecisionOutcome {
    Accepted,
    Declined,
}

impl StateInvalidation {
    pub fn new(vault_name: VaultName, scope: StateInvalidationScope) -> Self {
        Self {
            event_type: "state_invalidated",
            vault_name,
            scope: Some(scope),
            revision: None,
            join_request_id: None,
            join_result: None,
            e2e_signal: None,
        }
    }

    pub fn join_result(
        vault_name: VaultName,
        request_id: String,
        result: JoinResultOutcome,
    ) -> Self {
        Self {
            event_type: "join_result",
            vault_name,
            scope: None,
            revision: None,
            join_request_id: Some(request_id),
            join_result: Some(result),
            e2e_signal: None,
        }
    }

    pub fn recovery_decision(
        vault_name: VaultName,
        claim_id: String,
        outcome: RecoveryDecisionOutcome,
    ) -> Self {
        let mut invalidation = Self::new(vault_name, StateInvalidationScope::SsClaims);
        invalidation.e2e_signal = Some(E2eSignal {
            signal_type: "recovery_decision",
            claim_id,
            outcome,
        });
        invalidation
    }

    pub fn with_revision(mut self, revision: ArtifactId) -> Self {
        self.revision = Some(revision.id_str());
        self
    }
}

pub trait StateInvalidationPublisher: Send + Sync {
    fn publish(&self, invalidation: StateInvalidation);
}

#[derive(Default)]
pub struct NoopStateInvalidationPublisher;

impl StateInvalidationPublisher for NoopStateInvalidationPublisher {
    fn publish(&self, _invalidation: StateInvalidation) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use meta_secret_core::node::db::descriptors::object_descriptor::{ObjectFqdn, SeqId};

    #[test]
    fn serializes_minimal_state_invalidated_payload() {
        let invalidation =
            StateInvalidation::new(VaultName::from("vault-a"), StateInvalidationScope::SsClaims)
                .with_revision(ArtifactId {
                    fqdn: ObjectFqdn {
                        obj_type: "ss_log".to_string(),
                        obj_instance: "vault-a".to_string(),
                    },
                    id: SeqId::first(),
                });

        let json = serde_json::to_value(invalidation).unwrap();

        assert_eq!(json["type"], "state_invalidated");
        assert_eq!(json["vaultName"], "vault-a");
        assert_eq!(json["scope"], "ss_claims");
        assert_eq!(json["revision"], "ss_log:vault-a::1");
        assert!(json.get("secret").is_none());
        assert!(json.get("share").is_none());
        assert!(json.get("state").is_none());
    }
}
