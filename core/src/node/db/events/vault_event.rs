use crate::crypto::keys::OpenBox;
use crate::node::common::model::MetaPasswordId;
use crate::node::common::model::user::{UserDataCandidate, UserMembership};
use crate::node::common::model::vault::VaultData;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceObject {
    /// Devices' public key, to ensure that the only this device can send events to this log
    Unit {
        event: KvLogEvent<UnitId, OpenBox>
    },
    /// The only possible action for a new device that want's to join the vault is to send a JoinRequest
    JoinRequest {
        event: KvLogEvent<GenesisId, UserDataCandidate>,
    },
    /// When the device becomes a member of the vault, it can change membership of other members
    UpdateMembership {
        event: KvLogEvent<ArtifactId, UserMembership>,
    },
    /// Adds a new meta password into the vault
    AddMetaPassword {
        event: KvLogEvent<ArtifactId, MetaPasswordId>,
    }
}


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

    /// UserEvents or AuditEvents - the object contains user specific events, the reason is - when a device sends a join request, there is no way
    /// to say to the device what is the status of the user request, is pending or declined or anything else.
    /// Until the user joins the vault we need to maintain user specific table
    /// that contains the vault event for that particular user.
    VaultLog {
        event: KvLogEvent<ArtifactId, VaultObjectEvent>,
    },
}

impl ToGenericEvent for VaultObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::Vault(self)
    }
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
            VaultObject::Log { event } => ObjectId::from(event.key.obj_id.clone())
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
