use crate::node::db::events::error::LogEventCastError;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, ObjectId, VaultGenesisEvent, VaultUnitEvent};
use crate::node::db::events::vault_event::VaultActionEvent;
use anyhow::{anyhow, bail};

/// Each device has its own unique device_log table, to prevent conflicts in updates vault state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceLogObject {
    Unit(VaultUnitEvent),
    /// Device sends its data to ensure that the only this device can send events to this log
    Genesis(VaultGenesisEvent),
    Action(KvLogEvent<ArtifactId, VaultActionEvent>),
}

impl DeviceLogObject {
    pub fn get_unit(&self) -> anyhow::Result<VaultUnitEvent> {
        match self {
            DeviceLogObject::Unit(event) => Ok(event.clone()),
            _ => bail!(LogEventCastError::WrongDeviceLog(self.clone())),
        }
    }

    pub fn get_genesis(&self) -> anyhow::Result<VaultGenesisEvent> {
        match self {
            DeviceLogObject::Genesis(event) => Ok(event.clone()),
            _ => bail!(LogEventCastError::WrongDeviceLog(self.clone())),
        }
    }

    pub fn get_action(&self) -> anyhow::Result<VaultActionEvent> {
        match self {
            DeviceLogObject::Action(event) => Ok(event.value.clone()),
            _ => bail!(LogEventCastError::WrongDeviceLog(self.clone())),
        }
    }
}

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
    fn obj_id(&self) -> ObjectId {
        match self {
            DeviceLogObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            DeviceLogObject::Genesis(event) => ObjectId::from(event.key().obj_id.clone()),
            DeviceLogObject::Action(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl KeyExtractor for DeviceLogObject {
    fn key(&self) -> GenericKvKey {
        match self {
            DeviceLogObject::Unit(event) => GenericKvKey::from(event.key().clone()),
            DeviceLogObject::Genesis(event) => GenericKvKey::from(event.key().clone()),
            DeviceLogObject::Action(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}
