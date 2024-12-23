use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::user::common::{UserData, UserDataMember, UserMembership};
use crate::node::common::model::vault::{VaultData, VaultName};
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::error::LogEventCastError;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, GenesisId, Next, ObjectId, UnitId};
use anyhow::{anyhow, bail};
use std::fmt::Display;

use super::object_id::{VaultGenesisEvent, VaultUnitEvent};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultObject {
    Unit(VaultUnitEvent),
    /// Vault creator
    Genesis(KvLogEvent<GenesisId, DeviceData>),
    Vault(KvLogEvent<ArtifactId, VaultData>),
}

impl VaultObject {
    pub fn sign_up(vault_name: VaultName, candidate: UserData) -> Self {
        let desc = VaultDescriptor::vault(vault_name.clone());
        let vault_data = {
            let vault = VaultData::from(vault_name.clone());
            let membership = UserMembership::Member(UserDataMember {
                user_data: candidate.clone(),
            });
            vault.update_membership(membership)
        };

        let vault_id = UnitId::vault_unit(vault_name).next().next();

        let sign_up_event = KvLogEvent {
            key: KvKey::artifact(desc.clone(), vault_id),
            value: vault_data,
        };
        VaultObject::Vault(sign_up_event)
    }
}

impl VaultObject {
    pub fn genesis(vault_name: VaultName, server_device: DeviceData) -> Self {
        let desc = VaultDescriptor::vault(vault_name.clone());
        VaultObject::Genesis(KvLogEvent {
            key: KvKey::genesis(desc),
            value: server_device,
        })
    }
}

impl VaultObject {
    pub fn unit(vault_name: VaultName) -> Self {
        let desc = VaultDescriptor::vault(vault_name.clone());
        VaultObject::Unit(VaultUnitEvent(KvLogEvent {
            key: KvKey::unit(desc),
            value: vault_name,
        }))
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
    pub fn init(candidate: UserData) -> Vec<GenericKvLogEvent> {
        let unit_event = VaultMembershipObject::unit(candidate.clone()).to_generic();
        let genesis_event = VaultMembershipObject::genesis(candidate.clone()).to_generic();

        let member_event = {
            let desc = VaultDescriptor::VaultMembership(candidate.user_id()).to_obj_desc();
            let member_event_id = UnitId::unit(&desc).next().next();
            VaultMembershipObject::member(candidate, member_event_id).to_generic()
        };

        vec![unit_event, genesis_event, member_event]
    }

    fn unit(candidate: UserData) -> Self {
        let user_id = candidate.user_id();
        let desc = VaultDescriptor::VaultMembership(user_id).to_obj_desc();

        VaultMembershipObject::Unit(VaultUnitEvent(KvLogEvent {
            key: KvKey::unit(desc),
            value: candidate.vault_name,
        }))
    }

    pub fn genesis(candidate: UserData) -> Self {
        let desc = VaultDescriptor::VaultMembership(candidate.user_id()).to_obj_desc();

        VaultMembershipObject::Genesis(VaultGenesisEvent(KvLogEvent {
            key: KvKey::genesis(desc),
            value: candidate.clone(),
        }))
    }

    pub fn member(candidate: UserData, event_id: ArtifactId) -> Self {
        let member = UserMembership::Member(UserDataMember {
            user_data: candidate.clone(),
        });
        Self::membership(member, event_id)
    }

    pub fn membership(membership: UserMembership, event_id: ArtifactId) -> Self {
        let user_id = membership.user_data().user_id();
        let desc = VaultDescriptor::VaultMembership(user_id).to_obj_desc();

        VaultMembershipObject::Membership(KvLogEvent {
            key: KvKey {
                obj_id: event_id,
                obj_desc: desc,
            },
            value: membership,
        })
    }
}

impl VaultMembershipObject {
    pub fn is_member(&self) -> bool {
        let VaultMembershipObject::Membership(membership_event) = self else {
            return false;
        };

        matches!(
            membership_event.value,
            UserMembership::Member(UserDataMember { .. })
        )
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
pub enum VaultActionEvent {
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
    ActionCompleted {
        vault_name: VaultName,
    },
}

impl Display for VaultActionEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            VaultActionEvent::CreateVault(_) => String::from("CreateVault"),
            VaultActionEvent::JoinClusterRequest { .. } => String::from("JoinClusterRequest"),
            VaultActionEvent::UpdateMembership { .. } => String::from("UpdateMembership"),
            VaultActionEvent::AddMetaPassword { .. } => String::from("AddMetaPassword"),
            VaultActionEvent::ActionCompleted { .. } => String::from("ActionCompleted"),
        };
        write!(f, "{}", str)
    }
}

impl VaultActionEvent {
    pub fn get_create(self) -> anyhow::Result<UserData> {
        match self {
            VaultActionEvent::CreateVault(user) => Ok(user),
            _ => bail!(LogEventCastError::WrongVaultAction(
                String::from("CreateVault"),
                self.clone()
            )),
        }
    }

    pub fn get_join_request(self) -> anyhow::Result<UserData> {
        match self {
            VaultActionEvent::JoinClusterRequest { candidate } => Ok(candidate),
            _ => bail!(LogEventCastError::WrongVaultAction(
                String::from("JoinClusterRequest"),
                self.clone()
            )),
        }
    }

    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultActionEvent::JoinClusterRequest { candidate } => candidate.vault_name.clone(),
            VaultActionEvent::UpdateMembership { update, .. } => update.user_data().vault_name,
            VaultActionEvent::AddMetaPassword {
                sender: UserDataMember { user_data },
                ..
            } => user_data.vault_name.clone(),
            VaultActionEvent::CreateVault(user) => user.vault_name.clone(),
            VaultActionEvent::ActionCompleted { vault_name } => vault_name.clone(),
        }
    }
}
