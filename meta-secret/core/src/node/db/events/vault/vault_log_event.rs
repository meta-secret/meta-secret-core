use std::collections::HashMap;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::user::common::{UserData, UserDataMember, UserMembership};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultLogDescriptor;
use crate::node::db::events::error::LogEventCastError;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, ObjectId, VaultGenesisEvent, VaultUnitEvent};
use anyhow::{anyhow, bail, Result};
use std::fmt::Display;
use crate::crypto::utils::Id48bit;

/// VaultLog keeps incoming events in order, the log is a queue for incoming messages and used to
/// recreate the vault state from events (event sourcing)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultLogObject {
    Unit(VaultUnitEvent),
    Genesis(VaultGenesisEvent),
    Action(KvLogEvent<ArtifactId, VaultActionEvents>),
}

impl VaultLogObject {
    pub fn unit(vault_name: VaultName) -> Self {
        let desc = VaultLogDescriptor::from(vault_name.clone());

        VaultLogObject::Unit(VaultUnitEvent(KvLogEvent {
            key: KvKey::unit_from(desc),
            value: vault_name,
        }))
    }

    pub fn genesis(vault_name: VaultName, candidate: UserData) -> Self {
        let desc = VaultLogDescriptor::from(vault_name.clone());
        VaultLogObject::Genesis(VaultGenesisEvent(KvLogEvent {
            key: KvKey::genesis(desc.to_obj_desc()),
            value: candidate.clone(),
        }))
    }
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
            VaultLogObject::Unit(event) => GenericKvKey::from(event.key().clone()),
            VaultLogObject::Genesis(event) => GenericKvKey::from(event.key().clone()),
            VaultLogObject::Action(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl ObjIdExtractor for VaultLogObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultLogObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            VaultLogObject::Genesis(event) => ObjectId::from(event.key().obj_id.clone()),
            VaultLogObject::Action(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultActionEvents(pub HashMap<Id48bit, VaultActionEvent>);

impl VaultActionEvents {
    pub fn update(self, action_event: VaultActionEvent) -> Self {
        let events = self.0.into_iter().chain(vec![action_event]).collect();
        Self(events)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultActionEvent {
    Request(VaultActionRequestEvent),
    Update(VaultActionUpdateEvent),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultActionRequestEvent {
    JoinCluster {
        id: Id48bit,
        candidate: UserData
    },
}

impl VaultActionRequestEvent {
    pub fn name(&self) -> String {
        match self {
            VaultActionRequestEvent::JoinCluster { .. } => String::from("JoinRequest")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultActionUpdateEvent {
    CreateVault {
        id: Id48bit,
        owner: UserData,
    },

    /// When the device becomes a member of the vault, it can change membership of other members
    UpdateMembership {
        id: Id48bit,
        sender: UserDataMember,
        update: UserMembership,
    },
    /// A member can add a new meta password into the vault
    AddMetaPassword {
        id: Id48bit,
        sender: UserDataMember,
        meta_pass_id: MetaPasswordId,
    },
}

impl VaultActionUpdateEvent {
    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultActionUpdateEvent::CreateVault {owner, .. } => owner.vault_name(),
            VaultActionUpdateEvent::UpdateMembership { update, .. } => {
                update.user_data().vault_name
            }
            VaultActionUpdateEvent::AddMetaPassword { sender, .. } => {
                sender.user_data.vault_name()
            }
        }
    }
}

impl VaultActionUpdateEvent {
    pub fn name(&self) -> String {
        let name = match self {
            VaultActionUpdateEvent::CreateVault{ .. } => "CreateVaultAction",
            VaultActionUpdateEvent::UpdateMembership { .. } => "UpdateMembership",
            VaultActionUpdateEvent::AddMetaPassword { .. } => "AddMetaPassword",
        };
        
        name.to_string()
    }
}

impl VaultActionEvent {
    fn name(&self) -> String {
        match self {
            VaultActionEvent::Request(request) => request.name(),
            VaultActionEvent::Update(update) => update.name()
        }
    }
}

impl Display for VaultActionEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl VaultActionEvent {
    pub fn get_create(self) -> Result<UserData> {
        match self {
            VaultActionEvent::Update(VaultActionUpdateEvent::CreateVault { owner, .. }) => Ok(owner),
            _ => bail!(LogEventCastError::WrongVaultAction(
                String::from("CreateVault"),
                self.clone()
            )),
        }
    }

    pub fn get_join_request(self) -> Result<UserData> {
        if let VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster { candidate, .. }) = self
        {
            Ok(candidate)
        } else {
            bail!(LogEventCastError::WrongVaultAction(
                String::from("JoinClusterRequest"),
                self.clone()
            ))
        }
    }

    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster { candidate, .. }) => {
                candidate.vault_name()
            }
            VaultActionEvent::Update(update) => update.vault_name()
        }
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::db::events::vault::vault_log_event::{
        VaultActionEvents, VaultActionUpdateEvent,
    };

    #[test]
    fn test() {
        let fixture = FixtureRegistry::empty();

        let actions = VaultActionEvents::default();

        let actions = {
            let owner = fixture.state.user_creds.client.user();
            actions.update(VaultActionUpdateEvent::CreateVault { owner, .. })
        };

        assert_eq!(actions.0.len(), 1);
    }
}
