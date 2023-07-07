use crate::crypto::utils;
use crate::models::{Base64EncodedText, MetaVault, UserCredentials, UserSignature, VaultDoc};
use crate::node::db::events::object_id::{IdStr, ObjectId, IdGen};
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KvLogEventRequest {
    SignUp { event: KvLogEvent<UserSignature> },
    JoinCluster { event: KvLogEvent<UserSignature> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KvLogEventUpdate {
    Genesis { event: KvLogEvent<PublicKeyRecord> },
    GlobalIndex { event: KvLogEvent<GlobalIndexRecord> },
    SignUp { event: KvLogEvent<VaultDoc> },
    JoinCluster { event: KvLogEvent<VaultDoc> },
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
#[serde(tag = "event_type")]
pub enum GenericKvLogEvent {
    Request(KvLogEventRequest),
    Update(KvLogEventUpdate),
    LocalEvent(KvLogEventLocal),
    Error { event: KvLogEvent<ErrorMessage> },
}

pub trait LogEventKeyBasedRecord {
    fn key(&self) -> &KvKey;
}

impl LogEventKeyBasedRecord for GenericKvLogEvent {
    fn key(&self) -> &KvKey {
        match self {
            GenericKvLogEvent::Request(request) => match request {
                KvLogEventRequest::SignUp { event } => &event.key,
                KvLogEventRequest::JoinCluster { event } => &event.key,
            },
            GenericKvLogEvent::Update(op) => match op {
                KvLogEventUpdate::Genesis { event } => &event.key,
                KvLogEventUpdate::GlobalIndex { event } => &event.key,
                KvLogEventUpdate::SignUp { event } => &event.key,
                KvLogEventUpdate::JoinCluster { event } => &event.key,
            },
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
    pub fn formation(obj_desc: &ObjectDescriptor, server_pk: &PublicKeyRecord) -> KvLogEvent<PublicKeyRecord> {
        KvLogEvent {
            key: KvKey::unit(obj_desc),
            value: server_pk.clone(),
        }
    }

    pub fn global_index_formation(server_pk: &PublicKeyRecord) -> KvLogEvent<PublicKeyRecord> {
        Self::formation(&ObjectDescriptor::GlobalIndex, server_pk)
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
                vault_id: vault_id.id,
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
}

impl IdGen for KvKey {
    fn next(&self) -> Self {
        Self {
            obj_id: self.obj_id.next(),
            object_type: self.object_type,
        }
    }
}
