use anyhow::anyhow;

use crate::node::common::model::device::DeviceData;
use crate::node::common::model::secret::MetaPasswordId;
use crate::node::common::model::user::{UserData, UserDataMember, UserMembership};
use crate::node::common::model::vault::{VaultData, VaultName};
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};

/// Each device has its own unique device_log table, to prevent conflicts in updates vault state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceLogObject {
    Unit(KvLogEvent<UnitId, VaultName>),
    /// Device sends its data to ensure that the only this device can send events to this log
    Genesis(KvLogEvent<GenesisId, UserData>),
    Action(KvLogEvent<ArtifactId, VaultAction>),
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

impl ObjIdExtractor for DeviceLogObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            DeviceLogObject::Unit(event) => ObjectId::from(event.key.obj_id.clone()),
            DeviceLogObject::Genesis(event) => ObjectId::from(event.key.obj_id.clone()),
            DeviceLogObject::Action(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl KeyExtractor for DeviceLogObject {
    fn key(&self) -> GenericKvKey {
        match self {
            DeviceLogObject::Unit(event) => GenericKvKey::from(event.key.clone()),
            DeviceLogObject::Genesis(event) => GenericKvKey::from(event.key.clone()),
            DeviceLogObject::Action(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

/// VaultLog keeps incoming events in order, the log is a queue for incoming messages and used to
/// recreate the vault state from events (event sourcing)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultLogObject {
    Unit(KvLogEvent<UnitId, VaultName>),
    Genesis(KvLogEvent<GenesisId, UserData>),
    Action(KvLogEvent<ArtifactId, VaultAction>)
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

impl ToGenericEvent for VaultLogObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::VaultLog(self)
    }
}

impl KeyExtractor for VaultLogObject {
    fn key(&self) -> GenericKvKey {
        match self {
            VaultLogObject::Unit(event) => GenericKvKey::from(event.key.clone()),
            VaultLogObject::Genesis(event) => GenericKvKey::from(event.key.clone()),
            VaultLogObject::Action(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl ObjIdExtractor for VaultLogObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultLogObject::Unit(event) => ObjectId::from(event.key.obj_id.clone()),
            VaultLogObject::Genesis(event) => ObjectId::from(event.key.obj_id.clone()),
            VaultLogObject::Action(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultObject {
    Unit(KvLogEvent<UnitId, VaultName>),
    /// Meta Server public keys
    Genesis(KvLogEvent<GenesisId, DeviceData>),
    Vault(KvLogEvent<ArtifactId, VaultData>)
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

impl ToGenericEvent for VaultObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::Vault(self)
    }
}

impl ObjIdExtractor for VaultObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultObject::Unit(event) => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::Genesis(event) => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::Vault(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl KeyExtractor for VaultObject {
    fn key(&self) -> GenericKvKey {
        match self {
            VaultObject::Unit(event) => GenericKvKey::from(event.key.clone()),
            VaultObject::Genesis(event) => GenericKvKey::from(event.key.clone()),
            VaultObject::Vault(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultMembershipObject {
    Unit(KvLogEvent<UnitId, VaultName>),
    /// Device public keys
    Genesis(KvLogEvent<GenesisId, UserData>),
    Membership(KvLogEvent<ArtifactId, UserMembership>)
}

impl VaultMembershipObject {
    pub fn is_member(&self) -> bool {
        let VaultMembershipObject::Membership(membership_event) = self else {
            return false;
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

impl TryFrom<GenericKvLogEvent> for VaultMembershipObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::VaultMembership(vault_status) = event {
            Ok(vault_status)
        } else {
            Err(anyhow!("Not a vault status event"))
        }
    }
}

impl KeyExtractor for VaultMembershipObject {
    fn key(&self) -> GenericKvKey {
        match self {
            VaultMembershipObject::Unit(event) => GenericKvKey::from(event.key.clone()),
            VaultMembershipObject::Genesis(event) => GenericKvKey::from(event.key.clone()),
            VaultMembershipObject::Membership(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl ToGenericEvent for VaultMembershipObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::VaultMembership(self)
    }
}

impl ObjIdExtractor for VaultMembershipObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultMembershipObject::Unit(event) => ObjectId::from(event.key.obj_id.clone()),
            VaultMembershipObject::Genesis(event) => ObjectId::from(event.key.obj_id.clone()),
            VaultMembershipObject::Membership(event) => ObjectId::from(event.key.obj_id.clone()),
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
            VaultAction::JoinRequest { candidate } => candidate.vault_name.clone(),
            VaultAction::UpdateMembership { update, .. } => update.user_data().vault_name,
            VaultAction::AddMetaPassword { sender: UserDataMember(user), .. } => user.vault_name.clone()
        }
    }
}
