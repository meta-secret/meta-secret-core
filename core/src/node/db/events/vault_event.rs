use std::fmt::Display;
use anyhow::{anyhow, bail};
use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::secret::MetaPasswordId;
use crate::node::common::model::user::common::{UserData, UserDataMember, UserDataOutsider, UserMembership};
use crate::node::common::model::vault::{VaultData, VaultName, VaultStatus};
use crate::node::db::events::error::LogEventCastError;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId};

use super::object_id::{VaultGenesisEvent, VaultUnitEvent};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultObject {
    Unit(VaultUnitEvent),
    /// Meta Server public keys
    Genesis(KvLogEvent<GenesisId, DeviceData>),
    Vault(KvLogEvent<ArtifactId, VaultData>),
}

impl VaultObject {
    pub fn status(&self, user: UserData) -> VaultStatus {
        match self {
            VaultObject::Unit(_) => VaultStatus::Outsider(UserDataOutsider::non_member(user)),
            VaultObject::Genesis(_) => VaultStatus::Outsider(UserDataOutsider::non_member(user)),
            VaultObject::Vault(event) => {
                let vault = event.value.clone();
                vault.status(user)
            }
        }
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

impl ToGenericEvent for VaultObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::Vault(self)
    }
}

impl ObjIdExtractor for VaultObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            VaultObject::Genesis(event) => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::Vault(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl KeyExtractor for VaultObject {
    fn key(&self) -> GenericKvKey {
        match self {
            VaultObject::Unit(event) => GenericKvKey::from(event.key()),
            VaultObject::Genesis(event) => GenericKvKey::from(event.key.clone()),
            VaultObject::Vault(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultMembershipObject {
    Unit(VaultUnitEvent),
    /// Device public keys
    Genesis(VaultGenesisEvent),
    Membership(KvLogEvent<ArtifactId, UserMembership>),
}

impl VaultMembershipObject {
    pub fn is_member(&self) -> bool {
        let VaultMembershipObject::Membership(membership_event) = self else {
            return false;
        };

        matches!(membership_event.value, UserMembership::Member(UserDataMember { .. }))
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
            VaultMembershipObject::Unit(event) => GenericKvKey::from(event.key()),
            VaultMembershipObject::Genesis(event) => GenericKvKey::from(event.key()),
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
            VaultMembershipObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            VaultMembershipObject::Genesis(event) => ObjectId::from(event.key().obj_id.clone()),
            VaultMembershipObject::Membership(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultAction {
    CreateVault(UserData),

    JoinClusterRequest {
        candidate: UserData,
    },
    /// When the device becomes a member of the vault, it can change membership of other members
    UpdateMembership {
        sender: UserDataMember,
        update: UserMembership,
    },
    /// A member can add a new meta password into the vault
    AddMetaPassword {
        sender: UserDataMember,
        meta_pass_id: MetaPasswordId,
    },
}

impl Display for VaultAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            VaultAction::CreateVault(_) => String::from("CreateVault"),
            VaultAction::JoinClusterRequest { .. } => String::from("JoinClusterRequest"),
            VaultAction::UpdateMembership { .. } => String::from("UpdateMembership"),
            VaultAction::AddMetaPassword { .. } => String::from("AddMetaPassword"),
        };
        write!(f, "{}", str)
    }
}

impl VaultAction {
    pub fn get_create(self) -> anyhow::Result<UserData> {
        match self {
            VaultAction::CreateVault(user) => Ok(user),
            _ => bail!(LogEventCastError::WrongVaultAction(String::from("CreateVault"), self.clone())),
        }
    }

    pub fn get_join_request(self) -> anyhow::Result<UserData> {
        match self {
            VaultAction::JoinClusterRequest { candidate } => Ok(candidate),
            _ => bail!(LogEventCastError::WrongVaultAction(String::from("JoinClusterRequest"), self.clone())),
        }
    }

    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultAction::JoinClusterRequest { candidate } => candidate.vault_name.clone(),
            VaultAction::UpdateMembership { update, .. } => update.user_data().vault_name,
            VaultAction::AddMetaPassword {
                sender: UserDataMember(user),
                ..
            } => user.vault_name.clone(),
            VaultAction::CreateVault(user) => user.vault_name.clone(),
        }
    }
}
