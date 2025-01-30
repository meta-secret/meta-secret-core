use super::shared_secret_event::SsLogObject;
use crate::node::db::events::error::ErrorMessage;
use crate::node::db::events::global_index_event::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{GenericKvKey, KvLogEvent};
use crate::node::db::events::local_event::CredentialsObject;
use crate::node::db::events::object_id::{ArtifactId, ObjectId};
use crate::node::db::events::shared_secret_event::{SharedSecretObject, SsDeviceLogObject};
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::VaultLogObject;
use crate::node::db::events::vault::vault_membership::VaultMembershipObject;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenericKvLogEvent {
    GlobalIndex(GlobalIndexObject),

    Credentials(CredentialsObject),
    
    DeviceLog(DeviceLogObject),
    VaultLog(VaultLogObject),
    Vault(VaultObject),
    VaultMembership(VaultMembershipObject),

    SharedSecret(SharedSecretObject),
    SsDeviceLog(SsDeviceLogObject),
    SsLog(SsLogObject),

    DbError {
        event: KvLogEvent<ArtifactId, ErrorMessage>,
    },
}

impl GenericKvLogEvent {
    pub fn global_index(self) -> anyhow::Result<GlobalIndexObject> {
        GlobalIndexObject::try_from(self)
    }

    pub fn credentials(self) -> anyhow::Result<CredentialsObject> {
        CredentialsObject::try_from(self)
    }

    pub fn device_log(self) -> anyhow::Result<DeviceLogObject> {
        DeviceLogObject::try_from(self)
    }

    pub fn vault_log(self) -> anyhow::Result<VaultLogObject> {
        VaultLogObject::try_from(self)
    }

    pub fn vault(self) -> anyhow::Result<VaultObject> {
        VaultObject::try_from(self)
    }

    pub fn vault_membership(self) -> anyhow::Result<VaultMembershipObject> {
        VaultMembershipObject::try_from(self)
    }

    pub fn shared_secret(self) -> anyhow::Result<SharedSecretObject> {
        SharedSecretObject::try_from(self)
    }

    pub fn ss_device_log(self) -> anyhow::Result<SsDeviceLogObject> {
        SsDeviceLogObject::try_from(self)
    }

    pub fn ss_log(self) -> anyhow::Result<SsLogObject> {
        SsLogObject::try_from(self)
    }
}

pub trait GenericKvLogEventConvertible: Sized {
    fn try_from_event(event: GenericKvLogEvent) -> anyhow::Result<Self>;
}

impl GenericKvLogEventConvertible for GenericKvLogEvent {
    fn try_from_event(event: GenericKvLogEvent) -> anyhow::Result<Self> {
        Ok(event)
    }
}

impl<T> GenericKvLogEventConvertible for T
where
    T: TryFrom<GenericKvLogEvent, Error = anyhow::Error>,
{
    fn try_from_event(event: GenericKvLogEvent) -> anyhow::Result<Self> {
        T::try_from(event)
    }
}

pub trait ToGenericEvent: Clone {
    fn to_generic(self) -> GenericKvLogEvent;
}

impl ToGenericEvent for GenericKvLogEvent {
    fn to_generic(self) -> GenericKvLogEvent {
        self
    }
}

pub trait UnitEvent<T> {
    fn unit(value: T) -> Self;
}

pub trait UnitEventWithEmptyValue {
    fn unit() -> Self;
}

pub trait ObjIdExtractor {
    fn obj_id(&self) -> ObjectId;
}

pub trait KeyExtractor {
    fn key(&self) -> GenericKvKey;
}

impl ObjIdExtractor for GenericKvLogEvent {
    fn obj_id(&self) -> ObjectId {
        match self {
            GenericKvLogEvent::GlobalIndex(obj) => obj.obj_id(),
            GenericKvLogEvent::Vault(obj) => obj.obj_id(),
            GenericKvLogEvent::SharedSecret(obj) => obj.obj_id(),
            GenericKvLogEvent::Credentials(obj) => obj.obj_id(),
            GenericKvLogEvent::DbError { event } => ObjectId::from(event.key.obj_id.clone()),
            GenericKvLogEvent::DeviceLog(obj) => obj.obj_id(),
            GenericKvLogEvent::VaultLog(obj) => obj.obj_id(),
            GenericKvLogEvent::VaultMembership(obj) => obj.obj_id(),
            GenericKvLogEvent::SsDeviceLog(obj) => obj.obj_id(),
            GenericKvLogEvent::SsLog(obj) => obj.obj_id(),
        }
    }
}

impl KeyExtractor for GenericKvLogEvent {
    fn key(&self) -> GenericKvKey {
        match self {
            GenericKvLogEvent::GlobalIndex(obj) => obj.key(),
            GenericKvLogEvent::Vault(obj) => obj.key(),
            GenericKvLogEvent::SharedSecret(obj) => obj.key(),
            GenericKvLogEvent::Credentials(obj) => obj.key(),
            GenericKvLogEvent::DbError { event } => GenericKvKey::from(event.key.clone()),
            GenericKvLogEvent::DeviceLog(obj) => obj.key(),
            GenericKvLogEvent::VaultLog(obj) => obj.key(),
            GenericKvLogEvent::VaultMembership(obj) => obj.key(),
            GenericKvLogEvent::SsDeviceLog(obj) => obj.key(),
            GenericKvLogEvent::SsLog(obj) => obj.key(),
        }
    }
}
