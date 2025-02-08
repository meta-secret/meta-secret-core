use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::device::device_creds::DeviceCredentials;
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::db::descriptors::creds::CredentialsDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use anyhow::{anyhow, Error};
use crate::node::db::events::object_id::ArtifactId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialsObject {
    Device(KvLogEvent<DeviceCredentials>),
    /// Default vault
    DefaultUser(KvLogEvent<UserCredentials>),
}

impl From<DeviceCredentials> for CredentialsObject {
    fn from(creds: DeviceCredentials) -> Self {
        Self::Device(KvLogEvent {
            key: KvKey::from(CredentialsDescriptor::Device),
            value: creds,
        })
    }
}

impl From<UserCredentials> for CredentialsObject {
    fn from(creds: UserCredentials) -> Self {
        Self::DefaultUser(KvLogEvent {
            key: KvKey::from(CredentialsDescriptor::User),
            value: creds,
        })
    }
}

impl ObjIdExtractor for CredentialsObject {
    fn obj_id(&self) -> ArtifactId {
        match self {
            CredentialsObject::Device(event) => event.key.obj_id.clone(),
            CredentialsObject::DefaultUser(event) => event.key.obj_id.clone(),
        }
    }
}

impl ToGenericEvent for CredentialsObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::Credentials(self)
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
                creds_event.key().obj_desc
            );
            Err(error)
        }
    }
}

impl KeyExtractor for CredentialsObject {
    fn key(&self) -> KvKey {
        match self {
            CredentialsObject::Device(event) => event.key.clone(),
            CredentialsObject::DefaultUser(event) => event.key.clone(),
        }
    }
}

impl CredentialsObject {
    pub fn device(&self) -> DeviceData {
        match self {
            CredentialsObject::Device(event) => event.value.device.clone(),
            CredentialsObject::DefaultUser(event) => event.value.device_creds.device.clone(),
        }
    }
}
