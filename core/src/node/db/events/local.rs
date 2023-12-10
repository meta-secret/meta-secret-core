use anyhow::{anyhow, Error};
use crate::node::common::model::device::{DeviceCredentials, DeviceData};
use crate::node::common::model::user::UserCredentials;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent, UnitEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialsObject {
    Device {
        event: KvLogEvent<UnitId, DeviceCredentials>
    },
    /// Default vault
    DefaultUser {
        event: KvLogEvent<GenesisId, UserCredentials>
    }
}

impl ObjIdExtractor for CredentialsObject {
    fn obj_id(&self) -> ObjectId {
        CredentialsObject::unit_id()
    }
}

impl ToGenericEvent for CredentialsObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::Credentials(self)
    }
}

impl UnitEvent<DeviceCredentials> for CredentialsObject {
    fn unit(value: DeviceCredentials) -> Self {
        let event = KvLogEvent {
            key: KvKey::unit(ObjectDescriptor::CredsIndex),
            value,
        };

        CredentialsObject::Device { event }
    }
}

impl CredentialsObject {
    pub fn default_user(user: UserCredentials) -> Self {
        let event = KvLogEvent {
            key: KvKey::genesis(ObjectDescriptor::CredsIndex),
            value: user,
        };

        CredentialsObject::DefaultUser { event }
    }
}

impl TryFrom<GenericKvLogEvent> for CredentialsObject {
    type Error = Error;

    fn try_from(creds_event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::Credentials(creds_obj) = creds_event {
            return Ok(creds_obj);
        } else {
            let error: Error = anyhow!("Invalid credentials event type: {:?}", creds_event.key().obj_desc());
            Err(error)
        }
    }
}

impl CredentialsObject {
    pub fn unit_id() -> ObjectId {
        ObjectId::unit(ObjectDescriptor::CredsIndex)
    }

    pub fn device(&self) -> DeviceData {
        match self {
            CredentialsObject::Device { event } => event.value.device.clone(),
            CredentialsObject::DefaultUser { event } => event.value.device_creds.device.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTailObject {
    pub event: KvLogEvent<UnitId, DbTail>,
}

impl ToGenericEvent for DbTailObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::DbTail(self)
    }
}

impl ObjIdExtractor for DbTailObject {
    fn obj_id(&self) -> ObjectId {
        ObjectId::from(self.event.key.clone())
    }
}

impl UnitEvent<DbTail> for DbTailObject {
    fn unit(value: DbTail) -> Self {
        let key = KvKey::unit(ObjectDescriptor::DbTail);
        let event = KvLogEvent { key, value };
        Self { event }
    }
}
