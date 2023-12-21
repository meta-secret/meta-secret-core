use crate::node::db::descriptors::global_index::GlobalIndexDescriptor;
use crate::node::db::descriptors::shared_secret::SharedSecretDescriptor;
use crate::node::db::descriptors::vault::VaultDescriptor;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectDescriptor {
    DbTail,
    GlobalIndex(GlobalIndexDescriptor),
    /// Describes device and user credentials
    CredsIndex,

    Vault(VaultDescriptor),
    /// Secret distribution (split, recover, recovery request and so on)
    SharedSecret(SharedSecretDescriptor),
}

pub trait ToObjectDescriptor {
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
            ObjectDescriptor::DbTail => String::from("db_tail"),
            ObjectDescriptor::SharedSecret(s_s_descriptor) => s_s_descriptor.as_id_str(),
            ObjectDescriptor::GlobalIndex(desc) => desc.object_name(),
            ObjectDescriptor::CredsIndex => String::from("index"),
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

#[cfg(test)]
mod test {
    use serde_json::json;

    #[test]
    fn test_db_tail() -> anyhow::Result<()> {
        use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;

        let db_tail_json = {
            let db_tail = ObjectDescriptor::DbTail;
            serde_json::to_value(db_tail.fqdn())?
        };

        let expected_id = json!({
            "objType": "DbTail",
            "objInstance": "db_tail"
        });

        println!("db_tail_json: {}", db_tail_json);
        assert_eq!(expected_id, db_tail_json);

        Ok(())
    }
}