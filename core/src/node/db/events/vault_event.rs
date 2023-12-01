use anyhow::anyhow;
use crate::crypto::keys::OpenBox;
use crate::node::common::model::user::{UserDataCandidate, UserDataMember, UserMembership};
use crate::node::common::model::vault::{VaultData, VaultName};
use crate::node::common::model::MetaPasswordId;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};

/// Each device has its own unique device_log table, to prevent conflicts in updates vault state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceLogObject {
    /// Devices' public key, to ensure that the only this device can send events to this log
    Unit {
        event: KvLogEvent<UnitId, OpenBox>,
    },
    /// The only possible action for a new device that want's to join the vault is to send a JoinRequest
    JoinRequest {
        event: KvLogEvent<GenesisId, UserDataCandidate>,
    },
    Action {
        event: KvLogEvent<ArtifactId, VaultAction>,
    },
}

/// VaultLog keeps incoming events in order, the log is a queue for incoming messages and used to
/// recreate the vault state from events (event sourcing)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultLogObject {
    Unit {
        event: KvLogEvent<UnitId, OpenBox>,
    },
    /// The only possible action for a new vault that want's to join the vault is to send a JoinRequest
    JoinRequest {
        event: KvLogEvent<GenesisId, UserDataCandidate>,
    },
    Action {
        event: KvLogEvent<ArtifactId, VaultAction>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultObject {
    Unit {
        event: KvLogEvent<UnitId, VaultName>,
    },
    /// Meta Server public keys
    Genesis {
        event: KvLogEvent<GenesisId, OpenBox>,
    },
    Vault {
        event: KvLogEvent<ArtifactId, VaultData>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultStatusObject {
    Unit {
        event: KvLogEvent<UnitId, VaultName>,
    },
    /// Meta Server public keys
    Genesis {
        event: KvLogEvent<GenesisId, OpenBox>,
    },
    Status {
        event: KvLogEvent<ArtifactId, UserMembership>,
    },
}

impl VaultStatusObject {
    pub fn is_member(&self) -> bool {
        let VaultStatusObject::Status { event: membership_event } = self else {
            false
        };

        if let UserMembership::Member(UserDataMember { .. }) = membership_event.value {
            true
        } else {
            false
        }
    }

    pub fn is_not_member(&self) -> bool {
        !self.is_member()
    }
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
            VaultObjectEvent::UpdateMetaPassword { event } => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl ObjIdExtractor for VaultObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultObject::Unit { event } => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::Genesis { event } => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::Vault { event } => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultAction {
    /// When the device becomes a member of the vault, it can change membership of other members
    UpdateMembership { membership: UserMembership },
    /// Adds a new meta password into the vault
    AddMetaPassword { meta_pass_id: MetaPasswordId },
}
