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
    pub pk: Base64EncodedText,
}

impl From<Base64EncodedText> for PublicKeyRecord {
    fn from(value: Base64EncodedText) -> Self {
        Self { pk: value }
    }
}

pub trait ObjectCreator<T> {
    fn unit(value: T) -> Self;
    fn genesis(obj_desc: &ObjectDescriptor) -> Self;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__vault_ingo")]
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
