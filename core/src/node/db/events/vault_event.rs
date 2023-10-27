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
    JoinUpdate {
        event: KvLogEvent<ArtifactId, VaultData>,
    },
    JoinRequest {
        event: KvLogEvent<ArtifactId, UserDataCandidate>,
    },

    /// Contains user specific events, the reason is - when a device sends a join request, there is no way
    /// to say to the device what is the status of the user request, is pending or declined or anything else.
    /// Until the user joins the vault we need to maintain user specific table
    /// that contains the vault event for that particular user.
    UserEvents {
        event: KvLogEvent<ArtifactId, UserMembership>,
    }
}

impl ObjIdExtractor for VaultObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultObject::Unit { event } => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::Genesis { event } => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::JoinUpdate { event } => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::JoinRequest { event } => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::UserEvents { event } => ObjectId::from(event.key.obj_id.clone())
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
