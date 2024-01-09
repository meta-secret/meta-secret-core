use crate::node::common::model::secret::{PasswordRecoveryRequest, SecretDistributionData};
use crate::node::common::model::user::UserData;
use crate::node::common::model::vault::VaultName;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};
use anyhow::{anyhow, bail};

use super::kv_log_event::{KvKey, KvEvent};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum LogEventCastError {
    #[error("Invalid event")]
    InvalidEventType,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultUnitEvent(pub KvLogEvent<UnitId, VaultName>);

impl VaultUnitEvent {
    pub fn key(&self) -> KvKey<UnitId> {
        self.0.key.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultGenesisEvent(pub KvLogEvent<GenesisId, UserData>);

impl VaultGenesisEvent {
    pub fn key(&self) -> KvKey<GenesisId> {
        self.0.key.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretObject {
    LocalShare(KvLogEvent<UnitId, SecretDistributionData>),

    Split(KvLogEvent<UnitId, SecretDistributionData>),
    Recover(KvLogEvent<UnitId, SecretDistributionData>),

    SSLog(KvLogEvent<ArtifactId, ArtifactId>),
}

impl KeyExtractor for SharedSecretObject {
    fn key(&self) -> GenericKvKey {
        match self {
            SharedSecretObject::LocalShare(event) => GenericKvKey::from(event.key.clone()),

            SharedSecretObject::Split(event) => GenericKvKey::from(event.key.clone()),
            SharedSecretObject::Recover(event) => GenericKvKey::from(event.key.clone()),

            SharedSecretObject::SSLog(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl TryFrom<GenericKvLogEvent> for SharedSecretObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::SharedSecret(ss_obj) = event {
            Ok(ss_obj)
        } else {
            bail!(LogEventCastError::InvalidEventType)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SSDeviceLogObject {
    Unit(VaultUnitEvent),
    Genesis(VaultGenesisEvent),
    DeviceLog(KvLogEvent<ArtifactId, PasswordRecoveryRequest>),
}

impl SSDeviceLogObject {
    pub fn get_unit(&self) -> anyhow::Result<VaultUnitEvent> {
        match self {
            SSDeviceLogObject::Unit(event) => Ok(event.clone()),
            _ => bail!(LogEventCastError::InvalidEventType),
        }
    }

    pub fn get_genesis(&self) -> anyhow::Result<VaultGenesisEvent> {
        match self {
            SSDeviceLogObject::Genesis(event) => Ok(event.clone()),
            _ => bail!(LogEventCastError::InvalidEventType),
        }
    }

    pub fn get_recovery_request(&self) -> anyhow::Result<PasswordRecoveryRequest> {
        match self {
            SSDeviceLogObject::DeviceLog(event) => Ok(event.value.clone()),
            _ => bail!(LogEventCastError::InvalidEventType),
        }
    }
}

impl TryFrom<GenericKvLogEvent> for SSDeviceLogObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::SSDeviceLog(ss_device_log) = event {
            Ok(ss_device_log)
        } else {
            Err(anyhow!("Not a shared secret device log event"))
        }
    }
}

impl ObjIdExtractor for SSDeviceLogObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SSDeviceLogObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            SSDeviceLogObject::Genesis(event) => ObjectId::from(event.key().obj_id.clone()),
            SSDeviceLogObject::DeviceLog(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl ToGenericEvent for SSDeviceLogObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SSDeviceLog(self)
    }
}

impl KeyExtractor for SSDeviceLogObject {
    fn key(&self) -> GenericKvKey {
        match self {
            SSDeviceLogObject::Unit(event) => GenericKvKey::from(event.key()),
            SSDeviceLogObject::Genesis(event) => GenericKvKey::from(event.key()),
            SSDeviceLogObject::DeviceLog(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl ObjIdExtractor for SharedSecretObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SharedSecretObject::LocalShare(event) => ObjectId::from(event.key.obj_id.clone()),

            SharedSecretObject::Split(event) => ObjectId::from(event.key.obj_id.clone()),
            SharedSecretObject::Recover(event) => ObjectId::from(event.key.obj_id.clone()),

            SharedSecretObject::SSLog(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl ToGenericEvent for SharedSecretObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SharedSecret(self)
    }
}
