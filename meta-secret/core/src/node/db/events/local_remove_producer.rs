//! Scoped local 2→1 identity, distinct from guarded decision signing ownership.
use super::object_id::ArtifactId;
use crate::node::common::model::{
    device::common::DeviceId,
    liveboard_producer::{ProducerAttemptStage, ProducerMaterialStage},
    user::common::UserId,
};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRemoveProducerContext {
    pub owner: UserId,
    pub admission_vault: ArtifactId,
    pub target: DeviceId,
    pub request_stage: ProducerAttemptStage,
    pub decision_stage: Option<ProducerAttemptStage>,
    pub request_id: Option<String>,
    pub material_stage: ProducerMaterialStage,
}

impl std::fmt::Debug for LocalRemoveProducerContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocalRemoveProducerContext(<redacted>)")
    }
}
