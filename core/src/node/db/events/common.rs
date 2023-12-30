use crate::node::common::model::secret::{PasswordRecoveryRequest, SecretDistributionData};
use crate::node::common::model::user::UserData;
use crate::node::common::model::vault::VaultName;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SSDeviceLogObject {
    Unit(KvLogEvent<UnitId, VaultName>),
    Genesis(KvLogEvent<GenesisId, UserData>),
    DeviceLog(KvLogEvent<ArtifactId, PasswordRecoveryRequest>),
}

impl ObjIdExtractor for SSDeviceLogObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SSDeviceLogObject::Unit(event) => ObjectId::from(event.key.obj_id.clone()),
            SSDeviceLogObject::Genesis(event) => ObjectId::from(event.key.obj_id.clone()),
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
            SSDeviceLogObject::Unit(event) => GenericKvKey::from(event.key.clone()),
            SSDeviceLogObject::Genesis(event) => GenericKvKey::from(event.key.clone()),
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
