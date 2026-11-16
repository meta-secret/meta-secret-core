//! Strict nested decoding for optional output metadata, never command admission.
use super::*;
use crate::node::db::descriptors::object_descriptor::ObjectFqdn;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fqdn {
    obj_type: String,
    obj_instance: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sequence {
    curr: u64,
    prev: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    fqdn: Fqdn,
    id: Sequence,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Source {
    event_id: Id,
    body_commitment: String,
}

impl TryFrom<Source> for RevealSourceRef {
    type Error = &'static str;
    fn try_from(raw: Source) -> Result<Self, Self::Error> {
        let value = Self {
            event_id: ArtifactId {
                fqdn: ObjectFqdn {
                    obj_type: raw.event_id.fqdn.obj_type,
                    obj_instance: raw.event_id.fqdn.obj_instance,
                },
                id: serde_json::from_value(
                    serde_json::json!({"curr":raw.event_id.id.curr,"prev":raw.event_id.id.prev}),
                )
                .map_err(|_| "InvalidRevealMetadata")?,
            },
            body_commitment: raw.body_commitment,
        };
        if ["Vault", "SsDistribution", "SsRecovery"]
            .iter()
            .any(|kind| value.valid(kind))
        {
            Ok(value)
        } else {
            Err("InvalidRevealMetadata")
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Context {
    version: u32,
    vault_name: String,
    current_device_id: String,
    vault_source: RevealSourceRef,
    local_distribution_source: RevealSourceRef,
    recovery_sources: Vec<RevealSourceRef>,
    claim_scope_commitment: String,
}
impl TryFrom<Context> for LiveboardRevealContext {
    type Error = &'static str;
    fn try_from(raw: Context) -> Result<Self, Self::Error> {
        let value = Self {
            version: raw.version,
            vault_name: raw.vault_name,
            current_device_id: raw.current_device_id,
            vault_source: raw.vault_source,
            local_distribution_source: raw.local_distribution_source,
            recovery_sources: raw.recovery_sources,
            claim_scope_commitment: raw.claim_scope_commitment,
        };
        if value.is_valid() {
            Ok(value)
        } else {
            Err("InvalidRevealMetadata")
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Receipt {
    version: u32,
    secret_id: String,
    recovery_claim_id: String,
    requester_id: String,
    reveal_context: LiveboardRevealContext,
}
impl TryFrom<Receipt> for LiveboardRevealReceipt {
    type Error = &'static str;
    fn try_from(raw: Receipt) -> Result<Self, Self::Error> {
        let value = Self {
            version: raw.version,
            secret_id: raw.secret_id,
            recovery_claim_id: raw.recovery_claim_id,
            requester_id: raw.requester_id,
            reveal_context: raw.reveal_context,
        };
        if value.is_valid() {
            Ok(value)
        } else {
            Err("InvalidRevealMetadata")
        }
    }
}
