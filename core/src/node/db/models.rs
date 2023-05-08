use std::collections::HashSet;
use serde_json::Value;

use crate::crypto::utils;
use crate::models::{Base64EncodedText, VaultDoc};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaDb {
    pub vault_store: VaultStore,
    pub vaults: GlobalIndexStore
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
    pub tail_id: Option<KvKeyId>,
    pub global_index: HashSet<String>
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
pub enum AppOperation {
    #[serde(rename = "VaultFormation")]
    VaultFormation,
    #[serde(rename = "SignUp")]
    SignUp,
    #[serde(rename = "JoinCluster")]
    JoinCluster,
    #[serde(rename = "GlobalIndex")]
    GlobalIndexx,
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

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvKey {
    pub id: KvKeyId,
    pub store: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvKeyId {
    pub key_id: String,
    pub prev_key_id: String,
}

pub trait KeyIdGen {
    //fn genesis(vault_name: &str) -> Self;
    //fn genesis_from_vault_id(vault_id: &str) -> Self;

    fn object_foundation(vault_id: &str, store: &str) -> Self;

    fn next(&self) -> Self;
    fn generate_next(curr_id: &str) -> Self;

    fn from_prev_id(prev_id: &str) -> Self;
}

impl KeyIdGen for KvKeyId {

    fn object_foundation(object_id: &str, object_type: &str) -> Self {
        let full_name = format!("{}:{}", object_id, object_type);
        let id = utils::to_id(full_name.as_str()).base64_text;

        Self {
            key_id: id,
            prev_key_id: object_id.to_string(),
        }
    }

    fn next(&self) -> Self {
        let prev_id = self.key_id.as_str();
        let curr_id = utils::to_id(prev_id).base64_text;
        Self {
            key_id: curr_id,
            prev_key_id: prev_id.to_string(),
        }
    }

    fn generate_next(curr_id: &str) -> Self {
        let next_id = utils::to_id(curr_id).base64_text;
        Self {
            key_id: next_id,
            prev_key_id: curr_id.to_string(),
        }
    }

    fn from_prev_id(prev_id: &str) -> Self {
        let id = utils::to_id(prev_id).base64_text;
        Self {
            key_id: id,
            prev_key_id: prev_id.to_string(),
        }
    }
}
