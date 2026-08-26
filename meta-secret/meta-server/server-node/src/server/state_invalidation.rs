use meta_secret_core::node::common::model::IdString;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::events::object_id::ArtifactId;
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
}

impl StateInvalidation {
    pub fn new(vault_name: VaultName, scope: StateInvalidationScope) -> Self {
        Self {
            event_type: "state_invalidated",
            vault_name,
            scope: Some(scope),
            revision: None,
        }
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
