use crate::crypto::encoding::base64::Base64Text;
use crate::models::password_recovery_request::PasswordRecoveryRequest;
use crate::models::{Base64EncodedText, MetaPasswordDoc, SecretDistributionDocData, UserSignature, VaultDoc};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemPoolObject {
    JoinRequest { event: KvLogEvent<UserSignature> },
}

impl MemPoolObject {
    pub fn key(&self) -> &KvKey {
        match self {
            MemPoolObject::JoinRequest { event } => &event.key,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetaPassObject {
    Unit { event: KvLogEvent<()> },
    Genesis { event: KvLogEvent<PublicKeyRecord> },
    Update { event: KvLogEvent<MetaPasswordDoc> },
}

impl MetaPassObject {
    pub fn key(&self) -> &KvKey {
        match self {
            MetaPassObject::Unit { event } => &event.key,
            MetaPassObject::Genesis { event } => &event.key,
            MetaPassObject::Update { event } => &event.key,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretObject {
    Split {
        event: KvLogEvent<SecretDistributionDocData>,
    },
    Recover {
        event: KvLogEvent<SecretDistributionDocData>,
    },
    RecoveryRequest {
        event: KvLogEvent<PasswordRecoveryRequest>,
    },
    Audit {
        event: KvLogEvent<ObjectId>,
    },
}

impl SharedSecretObject {
    pub fn key(&self) -> &KvKey {
        match self {
            SharedSecretObject::Split { event } => &event.key,
            SharedSecretObject::Recover { event } => &event.key,
            SharedSecretObject::RecoveryRequest { event } => &event.key,
            SharedSecretObject::Audit { event } => &event.key,
        }
    }
}

pub trait LogEventKeyBasedRecord {
    fn key(&self) -> &KvKey;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyRecord {
    pub pk: Base64Text,
}

impl From<Base64Text> for PublicKeyRecord {
    fn from(value: Base64Text) -> Self {
        Self { pk: value }
    }
}

pub trait ObjectCreator<T> {
    fn unit(value: T) -> Self;
    fn genesis(obj_desc: &ObjectDescriptor) -> Self;
}

