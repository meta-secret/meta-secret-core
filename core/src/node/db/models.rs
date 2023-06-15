use std::collections::HashSet;

use serde_json::Value;

use crate::crypto::utils;
use crate::models::{Base64EncodedText, VaultDoc};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaDb {
    pub vault_store: VaultStore,
    pub global_index_store: GlobalIndexStore,
}

pub struct DbLog {
    pub events: Vec<KvLogEvent>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultStore {
    pub tail_id: Option<KvKeyId>,
    pub server_pk: Option<Base64EncodedText>,
    pub vault: Option<VaultDoc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GlobalIndexStore {
    pub server_pk: Option<Base64EncodedText>,
    pub tail_id: Option<KvKeyId>,
    pub global_index: HashSet<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum LogCommandError {
    #[error("Illegal message format: {err_msg:?}")]
    IllegalCommandFormat { err_msg: String },
    #[error("Illegal database state: {err_msg:?}")]
    IllegalDbState { err_msg: String },
    #[error(transparent)]
    JsonParseError {
        #[from]
        source: serde_json::Error,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum ObjectType {
    #[serde(rename = "Vault")]
    Vault,
    #[serde(rename = "GlobalIndex")]
    GlobalIndex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum AppOperation {
    #[serde(rename = "ObjectFormation")]
    ObjectFormation,
    #[serde(rename = "SignUp")]
    SignUp,
    #[serde(rename = "JoinCluster")]
    JoinCluster,
    #[serde(rename = "GlobalIndex")]
    GlobalIndex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum AppOperationType {
    #[serde(rename = "Request")]
    Request(AppOperation),
    #[serde(rename = "Update")]
    Update(AppOperation),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KvLogEvent {
    #[serde(rename = "key")]
    pub key: KvKey,
    #[serde(rename = "cmdType")]
    pub cmd_type: AppOperationType,
    #[serde(rename = "valType")]
    pub val_type: KvValueType,
    #[serde(rename = "value")]
    pub value: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum KvValueType {
    #[serde(rename = "DsaPublicKey")]
    DsaPublicKey,
    #[serde(rename = "UserSignature")]
    UserSignature,
    #[serde(rename = "Vault")]
    Vault,
    #[serde(rename = "String")]
    String,
    #[serde(rename = "Base64Text")]
    Base64Text,
    #[serde(rename = "Error")]
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvKey {
    pub key_id: KvKeyId,
    pub object_type: ObjectType,
}

impl ObjectCreator<&ObjectDescriptor> for KvKey {
    fn formation(obj_desc: &ObjectDescriptor) -> Self {
        Self {
            key_id: KvKeyId::formation(obj_desc),
            object_type: obj_desc.object_type,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvKeyId {
    pub obj_id: ObjectId,
    pub prev_obj_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectId {
    pub id: String,
    pub genesis_id: String,
}

pub struct Descriptors {}

impl Descriptors {
    pub fn global_index() -> ObjectDescriptor {
        ObjectDescriptor {
            name: String::from("meta-g"),
            object_type: ObjectType::GlobalIndex,
        }
    }
}

pub struct ObjectDescriptor {
    pub name: String,
    pub object_type: ObjectType,
}

impl ObjectDescriptor {
    pub fn to_id(&self) -> String {
        utils::to_id(self.full_name().as_str())
    }
}

impl ObjectDescriptor {
    pub fn full_name(&self) -> String {
        format!("{:?}:{}", self.object_type, self.name)
    }

    pub fn vault(name: &str) -> Self {
        Self {
            name: name.to_string(),
            object_type: ObjectType::Vault,
        }
    }

    pub fn global_index(name: &str) -> Self {
        Self {
            name: name.to_string(),
            object_type: ObjectType::GlobalIndex,
        }
    }
}

pub trait ObjectCreator<T> {
    fn formation(value: T) -> Self;
}

impl ObjectCreator<&ObjectDescriptor> for ObjectId {
    fn formation(obj_descriptor: &ObjectDescriptor) -> Self {
        let genesis_id = obj_descriptor.to_id();
        Self {
            id: genesis_id.clone(),
            genesis_id,
        }
    }
}

impl ObjectCreator<&ObjectDescriptor> for KvKeyId {
    fn formation(obj_descriptor: &ObjectDescriptor) -> Self {
        let obj_id = ObjectId::formation(obj_descriptor);
        KvKeyId::formation(obj_id.id)
    }
}

/// Build formation id based on genesis id
impl ObjectCreator<String> for KvKeyId {
    fn formation(genesis_id: String) -> Self {
        let origin_id = utils::to_id("meta-secret-genesis");
        Self {
            obj_id: ObjectId {
                id: genesis_id.clone(),
                genesis_id,
            },
            prev_obj_id: origin_id,
        }
    }
}

impl KvKeyId {
    pub fn vault_formation(vault_name: &str) -> Self {
        let vault_desc = ObjectDescriptor::vault(vault_name);
        KvKeyId::formation(&vault_desc)
    }
}

pub trait KeyIdGen {
    fn next(&self) -> Self;
}

impl KeyIdGen for KvKeyId {
    fn next(&self) -> Self {
        let curr_id = self.obj_id.id.clone();
        let next_id = utils::to_id(curr_id.as_str());

        let object_id = ObjectId {
            id: next_id,
            genesis_id: self.obj_id.genesis_id.clone(),
        };

        Self {
            obj_id: object_id,
            prev_obj_id: curr_id.clone(),
        }
    }
}

impl KeyIdGen for KvKey {
    fn next(&self) -> Self {
        Self {
            key_id: self.key_id.next(),
            object_type: self.object_type,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::node::db::models::{KvKeyId, ObjectCreator, ObjectDescriptor, ObjectType};

    #[test]
    fn test_key_id() {
        let descriptor = ObjectDescriptor {
            name: "test".to_string(),
            object_type: ObjectType::Vault,
        };
        let id = KvKeyId::formation(&descriptor);

        println!("{:?}", id);
    }
}
