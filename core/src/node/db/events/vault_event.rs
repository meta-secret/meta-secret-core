use crate::node::common::model::user::{UserDataCandidate, UserMembership};
use crate::node::common::model::vault::VaultData;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::generic_log_event::ObjIdExtractor;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultObject {
    /// SingUp request
    Unit {
        event: KvLogEvent<UnitId, UserDataCandidate>,
    },
    Genesis {
        event: KvLogEvent<GenesisId, PublicKeyRecord>,
    },

    Event(VaultObjectEvent),

    /// UserEvents or AuditEvents - the object contains user specific events, the reason is - when a device sends a join request, there is no way
    /// to say to the device what is the status of the user request, is pending or declined or anything else.
    /// Until the user joins the vault we need to maintain user specific table
    /// that contains the vault event for that particular user.
    Audit {
        event: KvLogEvent<ArtifactId, VaultObjectEvent>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultObjectEvent {
    JoinRequest {
        event: KvLogEvent<ArtifactId, UserDataCandidate>,
    },
    UpdateMembership {
        event: KvLogEvent<ArtifactId, VaultData>,
    },
    UpdateMetaPassword {
        event: KvLogEvent<ArtifactId, VaultData>,
    },
}

impl ObjIdExtractor for VaultObjectEvent {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultObjectEvent::JoinRequest { event } => ObjectId::from(event.key.obj_id.clone()),
            VaultObjectEvent::UpdateMembership { event } => ObjectId::from(event.key.obj_id.clone()),
            VaultObjectEvent::UpdateMetaPassword { event } => ObjectId::from(event.key.obj_id.clone())
        }
    }
}

impl ObjIdExtractor for VaultObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultObject::Unit { event } => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::Genesis { event } => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::Event(obj_event) => obj_event.obj_id(),
            VaultObject::Audit { event } => ObjectId::from(event.key.obj_id.clone())
        }
    }
}

impl VaultObject {
    pub fn unit(user_sig: UserDataCandidate) -> Self {
        VaultObject::Unit {
            event: KvLogEvent::vault_unit(user_sig),
        }
    }
}
