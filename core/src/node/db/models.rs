use std::string::ToString;

use crate::crypto::utils;
use crate::models::{Base64EncodedText, MetaPasswordDoc, MetaVault, UserCredentials, UserSignature, VaultDoc};
use crate::node::db::events::object_id::{IdGen, IdStr, ObjectId};

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

/// Local events (persistent objects which lives only in the local environment) which must not be synchronized
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KvLogEventLocal {
    MetaVault { event: Box<KvLogEvent<MetaVault>> },
    UserCredentials { event: Box<KvLogEvent<UserCredentials>> },
    DbTail { event: Box<KvLogEvent<DbTail>> },
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
    Unit {
        event: KvLogEvent<UserSignature>,
    },
    Genesis {
        event: KvLogEvent<PublicKeyRecord>,
    },

    SignUpUpdate {
        event: KvLogEvent<VaultDoc>,
    },

    JoinUpdate {
        event: KvLogEvent<VaultDoc>,
    },

    JoinRequest {
        event: KvLogEvent<UserSignature>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MempoolObject {
    JoinRequest { event: KvLogEvent<UserSignature> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetaPassObject {
    Unit { event: KvLogEvent<()> },
    Genesis { event: KvLogEvent<PublicKeyRecord> },
    Update { event: KvLogEvent<MetaPasswordDoc> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "event_type")]
pub enum GenericKvLogEvent {
    GlobalIndex(GlobalIndexObject),

    Vault(VaultObject),
    MetaPass(MetaPassObject),

    Mempool(MempoolObject),

    LocalEvent(KvLogEventLocal),

    Error { event: KvLogEvent<ErrorMessage> },
}

pub trait LogEventKeyBasedRecord {
    fn key(&self) -> &KvKey;
}

impl LogEventKeyBasedRecord for GenericKvLogEvent {
    fn key(&self) -> &KvKey {
        match self {
            GenericKvLogEvent::GlobalIndex(gi_obj) => match gi_obj {
                GlobalIndexObject::Unit { event } => &event.key,
                GlobalIndexObject::Genesis { event } => &event.key,
                GlobalIndexObject::Update { event } => &event.key,
            },
            GenericKvLogEvent::Vault(vault_obj) => match vault_obj {
                VaultObject::Unit { event } => &event.key,
                VaultObject::Genesis { event } => &event.key,
                VaultObject::SignUpUpdate { event } => &event.key,
                VaultObject::JoinUpdate { event } => &event.key,
                VaultObject::JoinRequest { event } => &event.key,
            },
            GenericKvLogEvent::MetaPass(pass_obj) => match pass_obj {
                MetaPassObject::Unit { event } => &event.key,
                MetaPassObject::Genesis { event } => &event.key,
                MetaPassObject::Update { event } => &event.key,
            },
            GenericKvLogEvent::Mempool(mem_pool_obj) => match mem_pool_obj {
                MempoolObject::JoinRequest { event } => &event.key,
            },
            GenericKvLogEvent::LocalEvent(op) => match op {
                KvLogEventLocal::DbTail { event } => &event.key,
                KvLogEventLocal::MetaVault { event } => &event.key,
                KvLogEventLocal::UserCredentials { event } => &event.key,
            },
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
    pub maybe_global_index_id: Option<ObjectId>,

    pub vault_id: DbTailObject,
    pub meta_pass_id: DbTailObject,

    pub maybe_mem_pool_id: Option<ObjectId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Hash, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DbTailObject {
    Empty { unit_id: ObjectId },
    Id { tail_id: ObjectId },
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
            obj_id: tail_id.clone(),
            obj_desc: ObjectDescriptor::GlobalIndex,
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
    pub obj_desc: ObjectDescriptor,
}

impl ObjectCreator<&ObjectDescriptor> for KvKey {
    fn unit(obj_desc: &ObjectDescriptor) -> Self {
        Self {
            obj_id: ObjectId::unit(obj_desc),
            obj_desc: obj_desc.clone(),
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
    Mempool,
    DbTail,
    Vault { vault_name: String },
    MetaPassword { vault_name: String },
    MetaVault,
    UserCreds,
}

impl ObjectDescriptor {
    pub fn to_id(&self) -> String {
        utils::to_id(self.full_name().as_str())
    }

    pub fn vault(vault_name: String) -> ObjectDescriptor {
        ObjectDescriptor::Vault { vault_name }
    }
}

impl ObjectDescriptor {
    pub fn full_name(&self) -> String {
        format!("{}:{}", self.to_string(), self.name())
    }

    pub fn name(&self) -> String {
        match self {
            ObjectDescriptor::GlobalIndex => String::from("meta-g"),
            ObjectDescriptor::Mempool => String::from("mem_pool"),

            ObjectDescriptor::DbTail => String::from("db_tail"),

            ObjectDescriptor::Vault { vault_name } => vault_name.clone(),
            ObjectDescriptor::MetaPassword { vault_name } => vault_name.clone(),

            ObjectDescriptor::MetaVault => String::from("main_meta_vault"),
            ObjectDescriptor::UserCreds => String::from("user_creds"),
        }
    }
}

impl ToString for ObjectDescriptor {
    fn to_string(&self) -> String {
        match self {
            ObjectDescriptor::GlobalIndex { .. } => String::from("GlobalIndex"),
            ObjectDescriptor::Mempool { .. } => String::from("Mempool"),

            ObjectDescriptor::Vault { .. } => String::from("Vault"),
            ObjectDescriptor::MetaPassword { .. } => String::from("MetaPass"),

            ObjectDescriptor::MetaVault { .. } => String::from("MetaVault"),
            ObjectDescriptor::UserCreds { .. } => String::from("UserCreds"),
            ObjectDescriptor::DbTail { .. } => String::from("DbTail"),
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
            obj_desc: self.obj_desc.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultInfo {
    /// Device is a member of a vault
    Member { vault: VaultDoc },
    /// Device is waiting to be added to a vault.
    Pending,
    /// Vault members declined to add a device into the vault.
    Declined,
    /// Vault not found
    NotFound,
    /// Device can't get any information about the vault, because its signature is not in members or pending list
    NotMember,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorMessage {
    stacktrace: Vec<String>,
}

impl From<&anyhow::Error> for ErrorMessage {
    fn from(err: &anyhow::Error) -> Self {
        let mut stacktrace = vec![];
        for cause in err.chain() {
            stacktrace.push(cause.to_string().trim().to_string());
        }

        Self { stacktrace }
    }
}

impl From<&dyn std::error::Error> for ErrorMessage {
    fn from(err: &dyn std::error::Error) -> Self {
        let mut stacktrace = vec![];

        let mut current_error = err;
        while let Some(source) = current_error.source() {
            let err_msg = format!("{}", current_error);
            stacktrace.push(err_msg);

            current_error = source;
        }

        Self { stacktrace }
    }
}
