use crate::node::db::descriptors::global_index::GlobalIndexDescriptor;
use crate::node::db::descriptors::shared_secret::SharedSecretDescriptor;
use crate::node::db::descriptors::vault::VaultDescriptor;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__obj_desc")]
pub enum ObjectDescriptor {
    DbTail,
    GlobalIndex(GlobalIndexDescriptor),
    /// Describes device and user credentials
    CredsIndex,

    Vault(VaultDescriptor),
    /// Secret distribution (split, recover, recovery request and so on)
    SharedSecret(SharedSecretDescriptor)
}
pub trait ObjectType {
    fn object_type(&self) -> String;
}

pub trait ObjectName {
    fn object_name(&self) -> String;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDescriptorFqdn {
    pub obj_type: String,
    pub obj_instance: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDescriptorId {
    pub fqdn: ObjectDescriptorFqdn,
    /// primary key of an object in the database in terms of keys in a table in relational databases.
    /// In our case id is just a counter
    pub id: usize,
}

impl ObjectDescriptor {
    pub fn to_fqdn(&self) -> ObjectDescriptorFqdn {
        self.fqdn()
    }
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
            ObjectDescriptor::DbTail => String::from("db_tail"),
            ObjectDescriptor::SharedSecret(s_s_descriptor) => s_s_descriptor.as_id_str(),
            ObjectDescriptor::GlobalIndex(desc) => desc.object_name(),
            ObjectDescriptor::CredsIndex => "index",
            ObjectDescriptor::Vault(vault_desc) => vault_desc.object_name(),
        }
    }
}

impl ObjectType for ObjectDescriptor {
    fn object_type(&self) -> String {
        match self {
            ObjectDescriptor::GlobalIndex(gi_desc) => gi_desc.object_type(),
            ObjectDescriptor::Vault(vault_desc) => vault_desc.object_type(),
            ObjectDescriptor::SharedSecret(ss_desc) => ss_desc.object_type(),
            ObjectDescriptor::CredsIndex { .. } => String::from("DeviceCreds"),
            ObjectDescriptor::DbTail { .. } => String::from("DbTail"),
        }
    }
}
