use std::string::ToString;

use crate::crypto::utils;
use crate::models::{Base64EncodedText, MetaVault, UserCredentials, UserSignature, VaultDoc};
use crate::node::db::events::object_id::{IdGen, IdStr, ObjectId};
use crate::sdk::api::ErrorMessage;

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
    GlobalIndexObj,
    VaultObj,

    Tail,
    MetaVaultObj,
    UserCreds,
}

/// Local events (persistent objects which lives only in the local environment) which must not be synchronized
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KvLogEventLocal {
    MetaVault { event: KvLogEvent<MetaVault> },
    UserCredentials { event: KvLogEvent<UserCredentials> },
    Tail { event: KvLogEvent<DbTail> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlobalIndexObject {
    Unit { event: KvLogEvent<()> },
    Genesis { event: KvLogEvent<PublicKeyRecord> },
    Update { event: KvLogEvent<GlobalIndexRecord> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultObject {
    /// SingUp request
    Unit { event: KvLogEvent<UserSignature> },
    Genesis { event: KvLogEvent<PublicKeyRecord> },

    SignUpUpdate { event: KvLogEvent<VaultDoc> },

    JoinUpdate { event: KvLogEvent<VaultDoc> },
    JoinRequest { event: KvLogEvent<UserSignature> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "event_type")]
pub enum GenericKvLogEvent {
    GlobalIndex(GlobalIndexObject),
    Vault(VaultObject),

    LocalEvent(KvLogEventLocal),

    Error { event: KvLogEvent<ErrorMessage> },
}

pub trait LogEventKeyBasedRecord {
    fn key(&self) -> &KvKey;
}

impl LogEventKeyBasedRecord for GenericKvLogEvent {
    fn key(&self) -> &KvKey {
        match self {
            GenericKvLogEvent::GlobalIndex(gi_obj) => {
                match gi_obj {
                    GlobalIndexObject::Unit { event } => { &event.key }
                    GlobalIndexObject::Genesis { event } => { &event.key }
                    GlobalIndexObject::Update { event } => { &event.key }
                }
            }
            GenericKvLogEvent::Vault(vault_obj) => {
                match vault_obj {
                    VaultObject::Unit { event } => { &event.key }
                    VaultObject::Genesis { event } => { &event.key }
                    VaultObject::SignUpUpdate { event } => { &event.key }
                    VaultObject::JoinUpdate { event } => { &event.key }
                    VaultObject::JoinRequest { event } => { &event.key }
                }
            }
            GenericKvLogEvent::LocalEvent(op) => match op {
                KvLogEventLocal::Tail { event } => &event.key,
                KvLogEventLocal::MetaVault { event } => &event.key,
                KvLogEventLocal::UserCredentials { event } => &event.key,
            }
            GenericKvLogEvent::Error { event } => &event.key,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyRecord {
    pub pk: Base64EncodedText,
}

impl From<Base64EncodedText> for PublicKeyRecord {
    fn from(value: Base64EncodedText) -> Self {
        Self { pk: value }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTail {
    pub vault: ObjectId,
    pub global_index: ObjectId,
}

impl Default for DbTail {
    fn default() -> Self {
        DbTail {
            vault: ObjectId::tail(),
            global_index: ObjectId::global_index(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalIndexRecord {
    pub vault_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvLogEvent<T> {
    pub key: KvKey,
    pub value: T,
}

impl KvLogEvent<PublicKeyRecord> {
    pub fn genesis(obj_desc: &ObjectDescriptor, server_pk: &PublicKeyRecord) -> KvLogEvent<PublicKeyRecord> {
        KvLogEvent {
            key: KvKey::genesis(obj_desc),
            value: server_pk.clone(),
        }
    }

    pub fn global_index_unit() -> KvLogEvent<()> {
        KvLogEvent {
            key: KvKey::unit(&ObjectDescriptor::GlobalIndex),
            value: (),
        }
    }

    pub fn global_index_genesis(server_pk: &PublicKeyRecord) -> KvLogEvent<PublicKeyRecord> {
        Self::genesis(&ObjectDescriptor::GlobalIndex, server_pk)
    }
}

impl KvLogEvent<GlobalIndexRecord> {
    pub fn new_global_index_event(tail_id: &ObjectId, vault_id: &IdStr) -> KvLogEvent<GlobalIndexRecord> {
        let key = KvKey {
            obj_id: tail_id.next(),
            object_type: ObjectType::GlobalIndexObj,
        };

        KvLogEvent {
            key,
            value: GlobalIndexRecord {
                vault_id: vault_id.id.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvKey {
    pub obj_id: ObjectId,
    pub object_type: ObjectType,
}

impl ObjectCreator<&ObjectDescriptor> for KvKey {
    fn unit(obj_desc: &ObjectDescriptor) -> Self {
        Self {
            obj_id: ObjectId::unit(obj_desc),
            object_type: ObjectType::from(obj_desc),
        }
    }

    fn genesis(obj_desc: &ObjectDescriptor) -> Self {
        Self::unit(obj_desc).next()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectDescriptor {
    GlobalIndex,
    Tail,
    Vault { name: String },
    MetaVault { name: String },
    UserCreds { name: String },
}

impl From<&ObjectDescriptor> for ObjectType {
    fn from(desc: &ObjectDescriptor) -> Self {
        match desc {
            ObjectDescriptor::GlobalIndex => { ObjectType::GlobalIndexObj }
            ObjectDescriptor::Vault { .. } => { ObjectType::VaultObj }
            ObjectDescriptor::MetaVault { .. } => { ObjectType::MetaVaultObj }
            ObjectDescriptor::UserCreds { .. } => { ObjectType::UserCreds }
            ObjectDescriptor::Tail => { ObjectType::Tail }
        }
    }
}

impl ObjectDescriptor {
    pub fn to_id(&self) -> String {
        utils::to_id(self.full_name().as_str())
    }
}

impl ObjectDescriptor {
    pub fn full_name(&self) -> String {
        format!("{}:{}", self.to_string(), self.name())
    }

    pub fn name(&self) -> String {
        match self {
            ObjectDescriptor::GlobalIndex => { String::from("meta-g") }
            ObjectDescriptor::Tail => { "db_tail".to_string() }

            ObjectDescriptor::Vault { name } => { name.clone() }
            ObjectDescriptor::MetaVault { name } => { name.clone() }
            ObjectDescriptor::UserCreds { name } => { name.clone() }
        }
    }
}

impl ToString for ObjectDescriptor {
    fn to_string(&self) -> String {
        match self {
            ObjectDescriptor::GlobalIndex { .. } => String::from("GlobalIndex"),
            ObjectDescriptor::Vault { .. } => String::from("Vault"),
            ObjectDescriptor::MetaVault { .. } => String::from("MetaVault"),
            ObjectDescriptor::UserCreds { .. } => String::from("UserCreds"),
            ObjectDescriptor::Tail { .. } => String::from("DbTail")
        }
    }
}

pub trait ObjectCreator<T> {
    fn unit(value: T) -> Self;
    fn genesis(obj_desc: &ObjectDescriptor) -> Self;
}

impl IdGen for KvKey {
    fn next(&self) -> Self {
        Self {
            obj_id: self.obj_id.next(),
            object_type: self.object_type,
        }
    }
}
