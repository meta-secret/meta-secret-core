use crate::node::common::model::IdString;
use crate::node::db::descriptors::creds::CredentialsDescriptor;
use crate::node::db::descriptors::global_index_descriptor::GlobalIndexDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::{
    SharedSecretDescriptor, SsDeviceLogDescriptor, SsLogDescriptor,
};
use crate::node::db::descriptors::vault_descriptor::{
    DeviceLogDescriptor, VaultDescriptor, VaultLogDescriptor, VaultMembershipDescriptor,
};
use crate::node::db::events::generic_log_event::GenericKvLogEventConvertible;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectDescriptor {
    GlobalIndex(GlobalIndexDescriptor),
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
pub struct ObjectDescriptorFqdn {
    pub obj_type: String,
    pub obj_instance: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDescriptorId {
    pub fqdn: ObjectDescriptorFqdn,
    /// primary key of an object in the database in terms of keys in a table in relational databases.
    /// In our case id is just a counter
    pub id: usize,
}

impl ObjectDescriptor {
    /// Fully Qualified Domain Name - unique domain name of an object
    pub fn fqdn(&self) -> ObjectDescriptorFqdn {
        ObjectDescriptorFqdn {
            obj_type: self.object_type(),
            obj_instance: self.object_name(),
        }
    }
}

impl ObjectName for ObjectDescriptor {
    fn object_name(&self) -> String {
        match self {
            ObjectDescriptor::GlobalIndex(desc) => desc.object_name(),
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
            ObjectDescriptor::GlobalIndex(gi_desc) => gi_desc.object_type(),
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
