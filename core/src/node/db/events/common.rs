use crate::crypto::encoding::base64::Base64Text;
use crate::node::common::model::{MetaPasswordData, PasswordRecoveryRequest, SecretDistributionDocData};
use crate::node::db::events::generic_log_event::ObjIdExtractor;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetaPassObject {
    Unit { event: KvLogEvent<UnitId, ()> },
    Genesis { event: KvLogEvent<GenesisId, PublicKeyRecord> },
    Update { event: KvLogEvent<ArtifactId, MetaPasswordData> },
}

impl ObjIdExtractor for MetaPassObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            MetaPassObject::Unit { event } => ObjectId::from(event.key.obj_id.clone()),
            MetaPassObject::Genesis { event } => ObjectId::from(event.key.obj_id.clone()),
            MetaPassObject::Update { event } => ObjectId::from(event.key.obj_id.clone())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretObject {
    Split {
        event: KvLogEvent<UnitId, SecretDistributionDocData>,
    },
    Recover {
        event: KvLogEvent<UnitId, SecretDistributionDocData>,
    },
    RecoveryRequest {
        event: KvLogEvent<UnitId, PasswordRecoveryRequest>,
    },
    Audit {
        event: KvLogEvent<ArtifactId, ArtifactId>,
    },
}

impl ObjIdExtractor for SharedSecretObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SharedSecretObject::Split { event } => ObjectId::from(event.key.obj_id.clone()),
            SharedSecretObject::Recover { event } => ObjectId::from(event.key.obj_id.clone()),
            SharedSecretObject::RecoveryRequest { event } => ObjectId::from(event.key.obj_id.clone()),
            SharedSecretObject::Audit { event } => ObjectId::from(event.key.obj_id.clone())
        }
    }
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

