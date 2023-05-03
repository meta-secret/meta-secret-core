use std::collections::HashSet;
use serde_json::Value;

use crate::crypto::utils;
use crate::models::{Base64EncodedText, VaultDoc};

pub const PRIMORDIAL: &str = "-1";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaDb {
    pub meta_store: MetaStore,
    pub vaults: VaultsStore
}

pub struct DbLog {
    pub events: Vec<KvLogEvent>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaStore {
    pub tail_id: Option<KvKeyId>,
    pub server_pk: Option<Base64EncodedText>,
    pub vault: Option<VaultDoc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultsStore {
    pub tail_id: Option<KvKeyId>,
    pub vaults_index: HashSet<String>
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
    #[serde(rename = "Genesis")]
    Genesis,
    #[serde(rename = "SignUp")]
    SignUp,
    #[serde(rename = "JoinCluster")]
    JoinCluster,
    #[serde(rename = "VaultsIndex")]
    VaultsIndex,
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
    fn genesis(vault_name: &str) -> Self;
    fn genesis_from_vault_id(vault_id: &str) -> Self;

    fn object_foundation(vault_id: &str, store: &str) -> Self;

    fn next(&self) -> Self;
    fn from_prev_id(prev_id: &str) -> Self;
}

impl KeyIdGen for KvKeyId {
    fn genesis(vault_name: &str) -> Self {
        let id = utils::to_id(vault_name).base64_text;
        Self::genesis_from_vault_id(id.as_str())
    }

    fn genesis_from_vault_id(vault_id: &str) -> Self {
        Self {
            key_id: vault_id.to_string(),
            prev_key_id: PRIMORDIAL.to_string(),
        }
    }

    fn object_foundation(vault_id: &str, object: &str) -> Self {
        let full_name = format!("{}:{}", vault_id, object);
        let id = utils::to_id(full_name.as_str()).base64_text;

        Self {
            key_id: id,
            prev_key_id: vault_id.to_string(),
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

    fn from_prev_id(prev_id: &str) -> Self {
        let id = utils::to_id(prev_id).base64_text;
        Self {
            key_id: id,
            prev_key_id: prev_id.to_string(),
        }
    }
}
