use anyhow::{bail, Result};
use crate::node::common::model::vault::vault::VaultName;
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
    Local(LocalKvLogEvent),
    Vault(VaultKvLogEvent),
    Ss(SsKvLogEvent),
    DbError(KvLogEvent<ErrorMessage>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SsKvLogEvent {
    SsDeviceLog(SsDeviceLogObject),
    SsLog(SsLogObject),
    SsWorkflow(SsWorkflowObject),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalKvLogEvent {
    DeviceCreds(DeviceCredsObject),
    UserCreds(UserCredsObject),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultKvLogEvent {
    DeviceLog(Box<DeviceLogObject>),
    VaultLog(VaultLogObject),
    Vault(VaultObject),
    VaultStatus(VaultStatusObject),
}

impl VaultKvLogEvent {
    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultKvLogEvent::DeviceLog(obj) => obj.0.value.vault_name(),
            VaultKvLogEvent::VaultLog(obj) => obj.0.value.vault_name.clone(),
            VaultKvLogEvent::Vault(obj) => obj.0.value.vault_name.clone(),
            VaultKvLogEvent::VaultStatus(obj) => obj.0.value.user().vault_name(),
        }
    }
}

impl SsKvLogEvent {
    pub fn vault_name(&self) -> VaultName {
        match self {
            SsKvLogEvent::SsDeviceLog(obj) => obj.0.value.vault_name.clone(),
            SsKvLogEvent::SsLog(obj) => obj.0.value.vault_name.clone(),
            SsKvLogEvent::SsWorkflow(obj) => obj.clone().to_distribution_data().vault_name.clone(),
        }
    }
}

impl GenericKvLogEvent {
    pub fn vault_name(&self) -> Result<VaultName> {
        match self {
            GenericKvLogEvent::Local(_) => {
                bail!("Wrong event type: {:?}", self);
            }
            GenericKvLogEvent::Vault(evt) => Ok(evt.vault_name()),
            GenericKvLogEvent::Ss(evt) => Ok(evt.vault_name()),
            GenericKvLogEvent::DbError(_) => {
                bail!("Wrong event type: {:?}", self);
            }
        }
    }
    
    pub fn device_creds(self) -> Result<DeviceCredsObject> {
        DeviceCredsObject::try_from(self)
    }

    pub fn user_creds(self) -> Result<UserCredsObject> {
        UserCredsObject::try_from(self)
    }

    pub fn device_log(self) -> Result<DeviceLogObject> {
        DeviceLogObject::try_from(self)
    }

    pub fn vault_log(self) -> Result<VaultLogObject> {
        VaultLogObject::try_from(self)
    }

    pub fn vault(self) -> Result<VaultObject> {
        VaultObject::try_from(self)
    }

    pub fn vault_membership(self) -> Result<VaultStatusObject> {
        VaultStatusObject::try_from(self)
    }

    pub fn shared_secret(self) -> Result<SsWorkflowObject> {
        SsWorkflowObject::try_from(self)
    }

    pub fn ss_device_log(self) -> Result<SsDeviceLogObject> {
        SsDeviceLogObject::try_from(self)
    }

    pub fn ss_log(self) -> Result<SsLogObject> {
        SsLogObject::try_from(self)
    }
}

pub trait GenericKvLogEventConvertible: Sized {
    fn try_from_event(event: GenericKvLogEvent) -> Result<Self>;
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
    fn try_from_event(event: GenericKvLogEvent) -> Result<Self> {
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
            GenericKvLogEvent::Local(local) => {
                match local {
                    LocalKvLogEvent::DeviceCreds(obj) => obj.obj_id(),
                    LocalKvLogEvent::UserCreds(obj) => obj.obj_id()
                }
            }
            GenericKvLogEvent::Vault(vault_kv) => {
                match vault_kv {
                    VaultKvLogEvent::DeviceLog(obj) => obj.obj_id(),
                    VaultKvLogEvent::VaultLog(obj) => obj.obj_id(),
                    VaultKvLogEvent::Vault(obj) => obj.obj_id(),
                    VaultKvLogEvent::VaultStatus(obj) => obj.obj_id(),
                }
            },
            GenericKvLogEvent::Ss(ss_kv) => {
                match ss_kv {
                    SsKvLogEvent::SsDeviceLog(obj) => obj.obj_id(),
                    SsKvLogEvent::SsLog(obj) => obj.obj_id(),
                    SsKvLogEvent::SsWorkflow(obj) => obj.obj_id(),
                }
            },
            GenericKvLogEvent::DbError(event) => event.key.obj_id.clone(),
        }
    }
}

impl KeyExtractor for GenericKvLogEvent {
    fn key(&self) -> KvKey {
        match self {
            GenericKvLogEvent::DbError(event) => event.key.clone(),
            GenericKvLogEvent::Local(local) => {
                match local {
                    LocalKvLogEvent::DeviceCreds(obj) => obj.key(),
                    LocalKvLogEvent::UserCreds(obj) => obj.key()
                }
            },
            GenericKvLogEvent::Vault(vault_kv) => {
                match vault_kv {
                    VaultKvLogEvent::DeviceLog(obj) => obj.key(),
                    VaultKvLogEvent::VaultLog(obj) => obj.key(),
                    VaultKvLogEvent::Vault(obj) => obj.key(),
                    VaultKvLogEvent::VaultStatus(obj) => obj.key(),
                }
            },
            GenericKvLogEvent::Ss(ss_kv) => {
                match ss_kv {
                    SsKvLogEvent::SsDeviceLog(obj) => obj.key(),
                    SsKvLogEvent::SsLog(obj) => obj.key(),
                    SsKvLogEvent::SsWorkflow(obj) => obj.key(),
                }
            }
        }
    }
}
