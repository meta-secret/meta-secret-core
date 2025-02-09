use crate::node::common::model::IdString;
use crate::node::db::descriptors::creds::CredentialsDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::{
    SharedSecretDescriptor, SsDeviceLogDescriptor, SsLogDescriptor,
};
use crate::node::db::descriptors::vault_descriptor::{
    DeviceLogDescriptor, VaultDescriptor, VaultLogDescriptor, VaultMembershipDescriptor,
};
use crate::node::db::events::generic_log_event::GenericKvLogEventConvertible;
use crate::node::db::events::object_id::Next;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectDescriptor {
    /// Describes device and user credentials
    Creds(CredentialsDescriptor),

    DeviceLog(DeviceLogDescriptor),

    VaultLog(VaultLogDescriptor),
    Vault(VaultDescriptor),
    VaultMembership(VaultMembershipDescriptor),

    /// Secret distribution (split, recover, recovery request and so on)
    SsLog(SsLogDescriptor),
    SsDeviceLog(SsDeviceLogDescriptor),
    SharedSecret(SharedSecretDescriptor),
}

pub trait ToObjectDescriptor: Clone {
    type EventType: GenericKvLogEventConvertible;
    fn to_obj_desc(self) -> ObjectDescriptor;
}

pub trait ObjectType {
    fn object_type(&self) -> String;
}

pub trait ObjectName {
    fn object_name(&self) -> String;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectFqdn {
    pub obj_type: String,
    pub obj_instance: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChainId {
    Genesis(GenesisId),
    Seq(SeqId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenesisId;

impl Next<SeqId> for GenesisId {
    fn next(self) -> SeqId {
        let curr = 0;
        SeqId {
            curr: curr + 1,
            prev: curr,
        }
    }
}

/// Sequential Id
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeqId {
    curr: usize,
    prev: usize,
}

impl SeqId {
    pub fn first() -> Self {
        GenesisId.next()
    }
}

impl Next<SeqId> for SeqId {
    fn next(mut self) -> SeqId {
        self.prev += 1;
        self.curr += 1;
        self
    }
}

impl ObjectDescriptor {
    /// Fully Qualified Domain Name - unique domain name of an object
    pub fn fqdn(&self) -> ObjectFqdn {
        ObjectFqdn {
            obj_type: self.object_type(),
            obj_instance: self.object_name(),
        }
    }
}

impl ObjectName for ObjectDescriptor {
    fn object_name(&self) -> String {
        match self {
            ObjectDescriptor::Creds(desc) => desc.object_name(),

            ObjectDescriptor::Vault(vault_desc) => vault_desc.object_name(),
            ObjectDescriptor::DeviceLog(device_log) => device_log.object_name(),
            ObjectDescriptor::VaultLog(vault_log) => vault_log.object_name(),
            ObjectDescriptor::VaultMembership(membership) => membership.object_name(),

            ObjectDescriptor::SharedSecret(s_s_descriptor) => s_s_descriptor.clone().id_str(),
            ObjectDescriptor::SsLog(desc) => desc.clone().id_str(),
            ObjectDescriptor::SsDeviceLog(desc) => desc.clone().id_str(),
        }
    }
}

impl ObjectType for ObjectDescriptor {
    fn object_type(&self) -> String {
        match self {
            ObjectDescriptor::Vault(vault_desc) => vault_desc.object_type(),
            ObjectDescriptor::SharedSecret(ss_desc) => ss_desc.object_type(),
            ObjectDescriptor::Creds(creds) => creds.object_type(),
            ObjectDescriptor::DeviceLog(device_log) => device_log.object_type(),
            ObjectDescriptor::VaultLog(vault_log) => vault_log.object_type(),
            ObjectDescriptor::VaultMembership(mem) => mem.object_type(),
            ObjectDescriptor::SsLog(desc) => desc.object_type(),
            ObjectDescriptor::SsDeviceLog(desc) => desc.object_type(),
        }
    }
}
