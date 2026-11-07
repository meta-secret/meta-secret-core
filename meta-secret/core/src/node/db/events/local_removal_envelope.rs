//! Local-only signing ownership. No Secret or encrypted transfer is stored here.
use super::{
    generic_log_event::{GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent},
    kv_log_event::{KvKey, KvLogEvent},
    object_id::ArtifactId,
};
use crate::node::common::model::user::common::UserId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRemovalEnvelopeObject(pub KvLogEvent<LocalRemovalEnvelopeRegistry>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRemovalEnvelopeRegistry {
    pub owner: UserId,
    pub first_managed_removal_nonce: u64,
    pub entries: Vec<LocalRemovalEnvelopeEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_context: Option<super::local_remove_producer::LocalRemoveProducerContext>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRemovalEnvelopeEntry {
    pub event_id: ArtifactId,
    pub mode: RemovalEnvelopeMode,
    pub request_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalEnvelopeMode {
    Guarded,
    Legacy,
}

impl ToGenericEvent for LocalRemovalEnvelopeObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::LocalRemovalEnvelope(self)
    }
}
impl TryFrom<GenericKvLogEvent> for LocalRemovalEnvelopeObject {
    type Error = anyhow::Error;
    fn try_from(event: GenericKvLogEvent) -> anyhow::Result<Self> {
        match event {
            GenericKvLogEvent::LocalRemovalEnvelope(value) => Ok(value),
            _ => anyhow::bail!("not a local removal envelope record"),
        }
    }
}
impl KeyExtractor for LocalRemovalEnvelopeObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}
impl ObjIdExtractor for LocalRemovalEnvelopeObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}
