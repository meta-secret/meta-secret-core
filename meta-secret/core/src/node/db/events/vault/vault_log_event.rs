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
use std::collections::HashSet;
use std::fmt::Display;
use tracing::info;

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

/// Represents all pending events to apply to the VaultObject
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultActionEvents {
    requests: HashSet<VaultActionRequestEvent>,
    updates: HashSet<VaultActionUpdateEvent>,
}

impl VaultActionEvents {
    pub fn update(mut self, event: VaultActionEvent) -> VaultActionEvents {
        match event {
            VaultActionEvent::Request(request) => {
                self.requests.insert(request);
                self
            }
            VaultActionEvent::Update(update) => {
                let request = match &update {
                    VaultActionUpdateEvent::CreateVault(event) => {
                        VaultActionRequestEvent::CreateVault(event.clone())
                    }
                    VaultActionUpdateEvent::UpdateMembership { request, .. } => {
                        VaultActionRequestEvent::JoinCluster(request.clone())
                    }
                    VaultActionUpdateEvent::AddMetaPass(event) => {
                        VaultActionRequestEvent::AddMetaPass(event.clone())
                    }
                };

                let removed = self.requests.remove(&request);
                // if corresponding request exists we can apply the update
                if removed {
                    self.updates.insert(update.clone());
                } else {
                    info!("Request not found: {:?}", request);
                }

                self
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultActionEvent {
    Request(VaultActionRequestEvent),
    Update(VaultActionUpdateEvent),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultActionRequestEvent {
    CreateVault(CreateVaultEvent),
    JoinCluster(JoinClusterEvent),
    AddMetaPass(AddMetaPassEvent),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVaultEvent {
    owner: UserData,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinClusterEvent {
    candidate: UserData,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMetaPassEvent {
    sender: UserDataMember,
    meta_pass_id: MetaPasswordId,
}

impl VaultActionRequestEvent {
    pub fn name(&self) -> String {
        let name = match self {
            VaultActionRequestEvent::JoinCluster { .. } => "JoinRequest",
            VaultActionRequestEvent::CreateVault { .. } => "CreateVaultRequest",
            VaultActionRequestEvent::AddMetaPass { .. } => "AddMetaPasswordRequest",
        };

        String::from(name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultActionUpdateEvent {
    CreateVault(CreateVaultEvent),

    /// When the device becomes a member of the vault, it can change membership of other members
    UpdateMembership {
        request: JoinClusterEvent,
        sender: UserDataMember,
        update: UserMembership,
    },
    /// A member can add a new meta password into the vault
    AddMetaPass(AddMetaPassEvent),
}

impl VaultActionUpdateEvent {
    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultActionUpdateEvent::CreateVault(CreateVaultEvent { owner, .. }) => {
                owner.vault_name()
            }
            VaultActionUpdateEvent::UpdateMembership { update, .. } => {
                update.user_data().vault_name
            }
            VaultActionUpdateEvent::AddMetaPass(AddMetaPassEvent { sender, .. }) => {
                sender.user_data.vault_name()
            }
        }
    }
}

impl VaultActionUpdateEvent {
    pub fn name(&self) -> String {
        let name = match self {
            VaultActionUpdateEvent::CreateVault(_) => "CreateVaultAction",
            VaultActionUpdateEvent::UpdateMembership { .. } => "UpdateMembership",
            VaultActionUpdateEvent::AddMetaPass { .. } => "AddMetaPassword",
        };

        name.to_string()
    }
}

impl VaultActionEvent {
    fn name(&self) -> String {
        match self {
            VaultActionEvent::Request(request) => request.name(),
            VaultActionEvent::Update(update) => update.name(),
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
        if let VaultActionEvent::Update(VaultActionUpdateEvent::CreateVault(upd_event)) = self {
            Ok(upd_event.owner.clone())
        } else {
            bail!(LogEventCastError::WrongVaultAction(
                String::from("CreateVault"),
                self.clone()
            ))
        }
    }

    pub fn get_join_request(self) -> Result<UserData> {
        if let VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(join_event)) = self {
            Ok(join_event.candidate)
        } else {
            bail!(LogEventCastError::WrongVaultAction(
                String::from("JoinClusterRequest"),
                self.clone()
            ))
        }
    }

    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(join_event)) => {
                join_event.candidate.vault_name()
            }
            VaultActionEvent::Update(update) => update.vault_name(),
            VaultActionEvent::Request(VaultActionRequestEvent::CreateVault(event)) => {
                event.owner.vault_name()
            }
            VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(event)) => {
                event.sender.user_data.vault_name()
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::db::events::vault::vault_log_event::{
        CreateVaultEvent, VaultActionEvent, VaultActionEvents, VaultActionRequestEvent,
        VaultActionUpdateEvent,
    };
    use anyhow::Result;

    #[test]
    fn test_create_vault_action_progression() -> Result<()> {
        let fixture = FixtureRegistry::empty();

        let create = CreateVaultEvent {
            owner: fixture.state.user_creds.client.user(),
        };
        let event = VaultActionEvent::Request(VaultActionRequestEvent::CreateVault(create.clone()));

        let empty = VaultActionEvents::default();
        let with_create_vault_request = empty.update(event);
        assert_eq!(with_create_vault_request.requests.len(), 1);

        let update = VaultActionUpdateEvent::CreateVault(create);
        let event = VaultActionEvent::Update(update);
        let with_update_vault_request = with_create_vault_request.update(event);
        assert_eq!(with_update_vault_request.requests.len(), 0);
        assert_eq!(with_update_vault_request.updates.len(), 1);

        Ok(())
    }
}
