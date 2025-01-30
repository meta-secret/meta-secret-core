use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::device::device_creds::DeviceCredentials;
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent, UnitEvent,
};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{GenesisId, ObjectId, UnitId};
use anyhow::{anyhow, Error};
use crate::node::db::descriptors::creds::CredentialsDescriptor;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialsObject {
    Device(KvLogEvent<UnitId, DeviceCredentials>),
    /// Default vault
    DefaultUser(KvLogEvent<GenesisId, UserCredentials>),
}

impl ObjIdExtractor for CredentialsObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            CredentialsObject::Device(event) => ObjectId::from(event.key.obj_id.clone()),
            CredentialsObject::DefaultUser(event) => ObjectId::from(event.key.obj_id.clone()),
        }
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
            key: KvKey::unit(ObjectDescriptor::Creds(CredentialsDescriptor {})),
            value,
        };

        CredentialsObject::Device(event)
    }
}

impl CredentialsObject {
    pub fn default_user(user: UserCredentials) -> Self {
        let event = KvLogEvent {
            key: KvKey::genesis(ObjectDescriptor::Creds(CredentialsDescriptor {})),
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
            let error: Error = anyhow!(
                "Invalid credentials event type: {:?}",
                creds_event.key().obj_desc()
            );
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
        ObjectId::unit(ObjectDescriptor::Creds(CredentialsDescriptor {}))
    }

    pub fn device(&self) -> DeviceData {
        match self {
            CredentialsObject::Device(event) => event.value.device.clone(),
            CredentialsObject::DefaultUser(event) => event.value.device_creds.device.clone(),
        }
    }
}
