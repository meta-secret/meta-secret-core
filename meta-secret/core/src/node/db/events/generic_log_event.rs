use super::shared_secret_event::SsLogObject;
use crate::node::db::events::error::ErrorMessage;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local_event::{DeviceCredsObject, UserCredsObject};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::shared_secret_event::{SsDeviceLogObject, SsWorkflowObject};
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::VaultLogObject;
use crate::node::db::events::vault::vault_status::VaultStatusObject;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenericKvLogEvent {
    DeviceCreds(DeviceCredsObject),
    UserCreds(UserCredsObject),

    DeviceLog(DeviceLogObject),
    VaultLog(VaultLogObject),
    Vault(VaultObject),
    VaultStatus(VaultStatusObject),

    SsDeviceLog(SsDeviceLogObject),
    SsLog(SsLogObject),
    SsWorkflow(SsWorkflowObject),

    DbError(KvLogEvent<ErrorMessage>),
}

impl GenericKvLogEvent {
    pub fn device_creds(self) -> anyhow::Result<DeviceCredsObject> {
        DeviceCredsObject::try_from(self)
    }

    pub fn user_creds(self) -> anyhow::Result<UserCredsObject> {
        UserCredsObject::try_from(self)
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

    pub fn vault_membership(self) -> anyhow::Result<VaultStatusObject> {
        VaultStatusObject::try_from(self)
    }

    pub fn shared_secret(self) -> anyhow::Result<SsWorkflowObject> {
        SsWorkflowObject::try_from(self)
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

pub trait ObjIdExtractor {
    fn obj_id(&self) -> ArtifactId;
}

pub trait KeyExtractor {
    fn key(&self) -> KvKey;
}

impl ObjIdExtractor for GenericKvLogEvent {
    fn obj_id(&self) -> ArtifactId {
        match self {
            GenericKvLogEvent::Vault(obj) => obj.obj_id(),
            GenericKvLogEvent::SsWorkflow(obj) => obj.obj_id(),
            GenericKvLogEvent::DeviceCreds(obj) => obj.obj_id(),
            GenericKvLogEvent::UserCreds(obj) => obj.obj_id(),
            GenericKvLogEvent::DbError(event) => event.key.obj_id.clone(),
            GenericKvLogEvent::DeviceLog(obj) => obj.obj_id(),
            GenericKvLogEvent::VaultLog(obj) => obj.obj_id(),
            GenericKvLogEvent::VaultStatus(obj) => obj.obj_id(),
            GenericKvLogEvent::SsDeviceLog(obj) => obj.obj_id(),
            GenericKvLogEvent::SsLog(obj) => obj.obj_id(),
        }
    }
}

impl KeyExtractor for GenericKvLogEvent {
    fn key(&self) -> KvKey {
        match self {
            GenericKvLogEvent::Vault(obj) => obj.key(),
            GenericKvLogEvent::SsWorkflow(obj) => obj.key(),
            GenericKvLogEvent::DeviceCreds(obj) => obj.key(),
            GenericKvLogEvent::UserCreds(obj) => obj.key(),
            GenericKvLogEvent::DbError(event) => event.key.clone(),
            GenericKvLogEvent::DeviceLog(obj) => obj.key(),
            GenericKvLogEvent::VaultLog(obj) => obj.key(),
            GenericKvLogEvent::VaultStatus(obj) => obj.key(),
            GenericKvLogEvent::SsDeviceLog(obj) => obj.key(),
            GenericKvLogEvent::SsLog(obj) => obj.key(),
        }
    }
}
