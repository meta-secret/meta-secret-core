use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::vault::vault_log_event::VaultActionEvent;
use anyhow::{Result, anyhow};

/// Each device has its own unique device_log table, to prevent conflicts in updates vault state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLogObject(pub KvLogEvent<VaultActionEvent>);

impl TryFrom<GenericKvLogEvent> for DeviceLogObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::DeviceLog(device_log) = event {
            Ok(device_log)
        } else {
            Err(anyhow!("Not a device log event"))
        }
    }
}

impl ToGenericEvent for DeviceLogObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::DeviceLog(self)
    }
}

impl ObjIdExtractor for DeviceLogObject {
    fn obj_id(&self) -> ArtifactId {
        self.key().obj_id.clone()
    }
}

impl KeyExtractor for DeviceLogObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}
