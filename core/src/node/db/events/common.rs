use crate::crypto::encoding::base64::Base64Text;
use crate::node::common::model::secret::{PasswordRecoveryRequest, SecretDistributionData};
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::{ArtifactId, ObjectId, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretObject {
    LocalShare {
        event: KvLogEvent<UnitId, SecretDistributionData>,
    },

    Split {
        event: KvLogEvent<UnitId, SecretDistributionData>,
    },
    Recover {
        event: KvLogEvent<UnitId, SecretDistributionData>,
    },
    
    SSDeviceLog {
        event: KvLogEvent<UnitId, PasswordRecoveryRequest>,
    },
    
    SSLog {
        event: KvLogEvent<ArtifactId, ArtifactId>,
    },
}

impl ObjIdExtractor for SharedSecretObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SharedSecretObject::LocalShare { event } => ObjectId::from(event.key.obj_id.clone()),

            SharedSecretObject::Split { event } => ObjectId::from(event.key.obj_id.clone()),
            SharedSecretObject::Recover { event } => ObjectId::from(event.key.obj_id.clone()),

            SharedSecretObject::SSDeviceLog { event } => ObjectId::from(event.key.obj_id.clone()),
            SharedSecretObject::SSLog { event } => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl ToGenericEvent for SharedSecretObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SharedSecret(self)
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

