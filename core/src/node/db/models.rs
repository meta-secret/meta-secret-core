use crate::crypto::utils;
use crate::models::{Base64EncodedText, MetaVault, UserCredentials, UserSignature, VaultDoc};
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "log_event_type")]
pub enum GenericKvLogEvent {
    Request(KvLogEventRequest),
    Update(KvLogEventUpdate),

    MetaVault { event: KvLogEvent<MetaVault> },
    UserCredentials { event: KvLogEvent<UserCredentials> },

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
            GenericKvLogEvent::MetaVault { event } => &event.key,
            GenericKvLogEvent::UserCredentials { event } => &event.key,
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
            key: KvKey::formation(obj_desc),
            value: server_pk.clone(),
        }
    }

    pub fn global_index_formation(server_pk: PublicKeyRecord) -> KvLogEvent<PublicKeyRecord> {
        KvLogEvent {
            key: Descriptors::global_index(),
            value: server_pk,
        }
    }
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
            object_type: ObjectType::from(obj_desc),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KvKeyId {
    GenesisKeyId {
        obj_id: ObjectId
    },
    RegularKeyId {
        obj_id: ObjectId,
        prev_obj_id: String,
    },
}

impl KvKeyId {
    pub fn obj_id(&self) -> ObjectId {
        match self {
            KvKeyId::GenesisKeyId { obj_id } => { obj_id.clone() }
            KvKeyId::RegularKeyId { obj_id, .. } => { obj_id.clone() }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectId {
    Genesis {
        id: String
    },
    Regular {
        id: String,
        genesis_id: String,
    },
}

impl ObjectId {
    pub fn genesis_id(&self) -> Self {
        match self {
            ObjectId::Genesis { .. } => {
                self.clone()
            }
            ObjectId::Regular { genesis_id, .. } => {
                Self::Genesis {
                    id: genesis_id.clone(),
                }
            }
        }
    }

    pub fn id_str(&self) -> String {
        match self {
            ObjectId::Genesis { id } => { id.clone() }
            ObjectId::Regular { id, .. } => { id.clone() }
        }
    }
}

pub struct Descriptors {}

impl Descriptors {
    pub fn global_index() -> ObjectDescriptor {
        ObjectDescriptor::GlobalIndex {
            name: String::from("meta-g")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectDescriptor {
    GlobalIndex { name: String },
    Vault { name: String },
    MetaVault { name: String },
    UserCreds { name: String },
}

impl From<&ObjectDescriptor> for ObjectType {
    fn from(desc: &ObjectDescriptor) -> Self {
        match desc {
            ObjectDescriptor::GlobalIndex { .. } => { ObjectType::GlobalIndexObj }
            ObjectDescriptor::Vault { .. } => { ObjectType::VaultObj }
            ObjectDescriptor::MetaVault { .. } => { ObjectType::MetaVaultObj }
            ObjectDescriptor::UserCreds { .. } => { ObjectType::UserCreds }
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
        let object_type = format!("{:?}", self);
        format!("{:?}:{}", object_type, self.name())
    }

    pub fn name(&self) -> String {
        match self {
            ObjectDescriptor::GlobalIndex { name } => { name.clone() }
            ObjectDescriptor::Vault { name } => { name.clone() }
            ObjectDescriptor::MetaVault { name } => { name.clone() }
            ObjectDescriptor::UserCreds { name } => { name.clone() }
        }
    }
}

pub trait ObjectCreator<T> {
    fn formation(value: T) -> Self;
}

impl ObjectCreator<&ObjectDescriptor> for ObjectId {
    fn formation(obj_descriptor: &ObjectDescriptor) -> Self {
        let genesis_id = obj_descriptor.to_id();
        Self::Genesis { id: genesis_id }
    }
}

impl ObjectCreator<&ObjectDescriptor> for KvKeyId {
    fn formation(obj_descriptor: &ObjectDescriptor) -> Self {
        let obj_id = ObjectId::formation(obj_descriptor);
        KvKeyId::formation(&obj_id)
    }
}

/// Build formation id based on genesis id
impl ObjectCreator<&ObjectId> for KvKeyId {
    fn formation(genesis_id: &ObjectId) -> Self {
        Self::GenesisKeyId {
            obj_id: genesis_id.genesis_id(),
        }
    }
}

impl KvKeyId {
    pub fn vault_formation(vault_name: &str) -> Self {
        let vault_desc = ObjectDescriptor::Vault { name: vault_name.to_string() };
        KvKeyId::formation(&vault_desc)
    }
}

pub trait KeyIdGen {
    fn next(&self) -> Self;
}

impl KeyIdGen for KvKeyId {
    fn next(&self) -> Self {
        let obj_id = self.obj_id();
        let curr_id_str = obj_id.clone().id_str();
        let next_id_str = utils::to_id(curr_id_str.as_str());

        let object_id = ObjectId::Regular {
            id: next_id_str,
            genesis_id: obj_id.genesis_id().id_str(),
        };

        KvKeyId::RegularKeyId {
            obj_id: object_id,
            prev_obj_id: curr_id_str.clone(),
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
    use crate::node::db::models::{KvKeyId, ObjectCreator, ObjectDescriptor};

    #[test]
    fn test_key_id() {
        let descriptor = ObjectDescriptor::Vault {
            name: "test".to_string(),
        };
        let id = KvKeyId::formation(&descriptor);

        println!("{:?}", id);
    }
}
