use anyhow::{anyhow, Error};

use crate::node::common::model::device::{DeviceCredentials, DeviceData};
use crate::node::common::model::user::UserCredentials;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent, UnitEvent,
};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{GenesisId, ObjectId, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialsObject {
    Device(KvLogEvent<UnitId, DeviceCredentials>),
    /// Default vault
    DefaultUser(KvLogEvent<GenesisId, UserCredentials>),
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

        CredentialsObject::Device(event)
    }
}

impl CredentialsObject {
    pub fn default_user(user: UserCredentials) -> Self {
        let event = KvLogEvent {
            key: KvKey::genesis(ObjectDescriptor::CredsIndex),
            value: user,
        };

        CredentialsObject::DefaultUser(event)
    }
}

impl TryFrom<GenericKvLogEvent> for CredentialsObject {
    type Error = Error;

    fn try_from(creds_event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::Credentials(creds_obj) = creds_event {
            Ok(creds_obj)
        } else {
            let error: Error = anyhow!("Invalid credentials event type: {:?}", creds_event.key().obj_desc());
            Err(error)
        }
    }
}

impl KeyExtractor for CredentialsObject {
    fn key(&self) -> GenericKvKey {
        match self {
            CredentialsObject::Device(event) => GenericKvKey::from(event.key.clone()),
            CredentialsObject::DefaultUser(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl CredentialsObject {
    pub fn unit_id() -> ObjectId {
        ObjectId::unit(ObjectDescriptor::CredsIndex)
    }

    pub fn device(&self) -> DeviceData {
        match self {
            CredentialsObject::Device(event) => event.value.device.clone(),
            CredentialsObject::DefaultUser(event) => event.value.device_creds.device.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTailObject(pub KvLogEvent<UnitId, DbTail>);

impl ToGenericEvent for DbTailObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::DbTail(self)
    }
}

impl KeyExtractor for DbTailObject {
    fn key(&self) -> GenericKvKey {
        GenericKvKey::from(self.0.key.clone())
    }
}

impl ObjIdExtractor for DbTailObject {
    fn obj_id(&self) -> ObjectId {
        ObjectId::from(self.0.key.obj_id.clone())
    }
}

impl UnitEvent<DbTail> for DbTailObject {
    fn unit(value: DbTail) -> Self {
        let key = KvKey::unit(ObjectDescriptor::DbTail);
        let event = KvLogEvent { key, value };
        Self(event)
    }
}
