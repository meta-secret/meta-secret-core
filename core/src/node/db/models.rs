use serde_json::Value;
use std::collections::HashSet;

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
    pub id: KvKeyId,
    pub object_type: ObjectType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvKeyId {
    pub key_id: String,
    pub prev_key_id: String,
}

pub struct VaultId {
    pub vault_id: String
}

impl VaultId {
    pub fn build(object_name: &str, object_type: ObjectType) -> Self {
        let full_name = format!("{:?}:{}", object_type, object_name);
        let vault_id = utils::to_id(full_name.as_str());
        Self { vault_id }
    }
}


pub trait KeyIdGen {
    fn object_foundation_from_id(object_id: &str) -> Self;
    fn object_foundation(object_id: &str, object_type: ObjectType) -> Self;

    fn next(&self) -> Self;
    fn generate_next(curr_id: &str) -> Self;
}

impl KeyIdGen for KvKeyId {
    fn object_foundation_from_id(object_id: &str) -> Self {
        let prev_key_id = utils::to_id("meta-secret-genesis");
        Self { key_id: object_id.to_string(), prev_key_id }
    }

    fn object_foundation(object_name: &str, object_type: ObjectType) -> Self {
        let vault_id = VaultId::build(object_name, object_type);
        Self::object_foundation_from_id(vault_id.vault_id.as_str())
    }

    fn next(&self) -> Self {
        let curr_id = self.key_id.as_str();
        let next_id = utils::to_id(curr_id);
        Self {
            key_id: next_id,
            prev_key_id: curr_id.to_string(),
        }
    }

    fn generate_next(curr_id: &str) -> Self {
        let next_id = utils::to_id(curr_id);
        Self {
            key_id: next_id,
            prev_key_id: curr_id.to_string(),
        }
    }
}


#[cfg(test)]
mod test {
    use crate::node::db::models::{KeyIdGen, KvKeyId, ObjectType};

    #[test]
    fn test_key_id() {
        let id = KvKeyId::object_foundation("test", ObjectType::Vault);
        println!("{:?}", id);
    }
}