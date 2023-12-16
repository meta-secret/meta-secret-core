use anyhow::anyhow;
use crate::node::common::model::device::DeviceData;
use crate::node::common::model::user::{UserData, UserDataCandidate, UserDataMember, UserDataOutsider, UserMembership};
use crate::node::common::model::vault::{VaultData, VaultName, VaultStatus};
use crate::node::common::model::MetaPasswordId;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};

/// Each device has its own unique device_log table, to prevent conflicts in updates vault state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceLogObject {
    Unit {
        event: KvLogEvent<UnitId, VaultName>,
    },
    /// Device sends its data to ensure that the only this device can send events to this log
    Genesis {
        event: KvLogEvent<GenesisId, UserData>,
    },
    Action {
        event: KvLogEvent<ArtifactId, VaultAction>,
    },
}

impl TryFrom<GenericKvLogEvent> for DeviceLogObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::DeviceLog(device_log) = event {
            Ok(device_log)
        } else {
            Err(anyhow!("Not a device log event"))
        }
    }
}

impl ToGenericEvent for DeviceLogObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::DeviceLog(self)
    }
}

/// VaultLog keeps incoming events in order, the log is a queue for incoming messages and used to
/// recreate the vault state from events (event sourcing)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultLogObject {
    Unit {
        event: KvLogEvent<UnitId, VaultName>,
    },
    Genesis {
        event: KvLogEvent<GenesisId, UserDataCandidate>,
    },
    Action {
        event: KvLogEvent<ArtifactId, VaultAction>,
    },
}

impl TryFrom<GenericKvLogEvent> for VaultLogObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::VaultLog(vault_log) = event {
            Ok(vault_log)
        } else {
            Err(anyhow!("Not a vault log event"))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultObject {
    Unit {
        event: KvLogEvent<UnitId, VaultName>,
    },
    /// Meta Server public keys
    Genesis {
        event: KvLogEvent<GenesisId, DeviceData>,
    },
    Vault {
        event: KvLogEvent<ArtifactId, VaultData>,
    }
}

impl TryFrom<GenericKvLogEvent> for VaultObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::Vault(vault) = event {
            Ok(vault)
        } else {
            Err(anyhow!("Not a vault event"))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultStatusObject {
    Unit {
        event: KvLogEvent<UnitId, VaultName>,
    },
    /// Device public keys
    Genesis {
        event: KvLogEvent<GenesisId, UserData>,
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

impl TryFrom<GenericKvLogEvent> for VaultStatusObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::VaultStatus(vault_status) = event {
            Ok(vault_status)
        } else {
            Err(anyhow!("Not a vault status event"))
        }
    }
}

impl ToGenericEvent for VaultObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::Vault(self)
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
    JoinRequest { candidate: UserData },
    /// When the device becomes a member of the vault, it can change membership of other members
    UpdateMembership {
        sender: UserDataMember,
        update: UserMembership
    },
    /// A member can add a new meta password into the vault
    AddMetaPassword {
        sender: UserDataMember,
        meta_pass_id: MetaPasswordId
    },
}

impl VaultAction {
    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultAction::JoinRequest { candidate } => candidate.user_data.vault_name.clone(),
            VaultAction::UpdateMembership { update, .. } => update.user_data().vault_name.clone(),
            VaultAction::AddMetaPassword { sender, .. } => sender.user_data.vault_name.clone(),
        }
    }
}
