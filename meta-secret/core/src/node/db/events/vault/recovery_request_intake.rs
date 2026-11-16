//! B2 compile schema: unsigned metadata for one original initial source intake.
use crate::node::{
    common::model::{device::common::DeviceId, secret::ClaimId},
    db::events::{kv_log_event::KvKey, object_id::ArtifactId},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRequestIntake {
    pub version: u32,
    pub claim_id: ClaimId,
    pub requester_id: DeviceId,
    pub source_key: KvKey,
    pub source_body_hash: String,
    pub verified_action_hash: String,
    pub verified_vault_artifact_id: ArtifactId,
    pub base_ss_log_artifact_id: Option<ArtifactId>,
    pub target_ss_log_artifact_id: ArtifactId,
}
// KvKey is metadata-only (typed descriptors and opaque IDs), with reflexive equality.
impl Eq for RecoveryRequestIntake {}
