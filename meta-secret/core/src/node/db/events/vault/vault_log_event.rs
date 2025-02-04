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
use derive_more::From;
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
    
    pub fn get_events(&self) -> Result<VaultActionEvents> {
        let VaultLogObject::Action(action) = self else {
            bail!("Expected an action event");
        };
        
        Ok(action.value.clone())
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
    pub requests: HashSet<VaultActionRequestEvent>,
    pub updates: HashSet<VaultActionUpdateEvent>,
}

impl VaultActionEvents {

    pub fn synchronize(mut self) -> Self {
        let updates = self.updates.clone();

        for update in updates {
            self = self.apply_event(VaultActionEvent::Update(update));
        }

        self
    }

    /// Take all vault action events and update vault with those events, then return updated vault
    pub fn apply_event(mut self, event: VaultActionEvent) -> Self {
        match event {
            VaultActionEvent::Request(request) => {
                self = self.add(request);
            }
            VaultActionEvent::Update(update) => {
                self = self.apply(update);
            }
            VaultActionEvent::Init(_) => {
                //no op
            }
        }

        self
    }

    pub fn add(mut self, request: VaultActionRequestEvent) -> Self {
        self.requests.insert(request);
        self
    }

    pub fn apply(mut self, event: VaultActionUpdateEvent) -> Self {
        let request = match &event {
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
            self.updates.insert(event.clone());
        } else {
            info!("Corresponding request not found: {:?}, update won't be applied", request);
        }

        self
    }

    /// Completing vault action update events, which means the updates has been applied to the VaultObject
    /// and needs to be removed from the updates list
    pub fn complete(mut self) -> Self {
        self.updates.clear();
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultActionEvent {
    Init(VaultActionInitEvent),
    Request(VaultActionRequestEvent),
    Update(VaultActionUpdateEvent),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultActionInitEvent {
    CreateVault(CreateVaultEvent),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultActionRequestEvent {
    JoinCluster(JoinClusterEvent),
    AddMetaPass(AddMetaPassEvent),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVaultEvent {
    pub owner: UserData,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinClusterEvent {
    pub candidate: UserData,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMetaPassEvent {
    pub sender: UserDataMember,
    pub meta_pass_id: MetaPasswordId,
}

impl VaultActionRequestEvent {
    pub fn name(&self) -> String {
        let name = match self {
            VaultActionRequestEvent::JoinCluster { .. } => "JoinRequest",
            VaultActionRequestEvent::AddMetaPass { .. } => "AddMetaPasswordRequest",
        };

        String::from(name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultActionUpdateEvent {
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
            VaultActionUpdateEvent::UpdateMembership { update, .. } => {
                update.user_data().vault_name
            }
            VaultActionUpdateEvent::AddMetaPass(AddMetaPassEvent { sender, .. }) => {
                sender.user_data.vault_name()
            }
        }
    }
}

impl VaultActionRequestEvent {
    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultActionRequestEvent::JoinCluster(request) => request.candidate.vault_name(),
            VaultActionRequestEvent::AddMetaPass(request) => request.sender.user_data.vault_name()
        }
    }
}

impl VaultActionUpdateEvent {
    pub fn name(&self) -> String {
        let name = match self {
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
            VaultActionEvent::Init(_) => "CreateVaultRequest".to_string()
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
        if let VaultActionEvent::Init(VaultActionInitEvent::CreateVault(upd_event)) = self {
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
            VaultActionEvent::Init(VaultActionInitEvent::CreateVault(event)) => {
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
    use crate::node::common::model::user::common::{UserDataMember, UserMembership};
    use crate::node::db::events::vault::vault_log_event::{JoinClusterEvent, VaultActionEvent, VaultActionEvents, VaultActionRequestEvent, VaultActionUpdateEvent};
    use anyhow::Result;

    #[test]
    fn test_create_vault_action_progression() -> Result<()> {
        let fixture = FixtureRegistry::empty();
        let client_creds = fixture.state.user_creds.client;
        let client_b_creds = fixture.state.user_creds.client_b;

        let join_request = JoinClusterEvent {
            candidate: client_creds.user(),
        };
        let event = VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(join_request.clone()));

        let actions = VaultActionEvents::default()
            .apply_event(event);
        assert_eq!(actions.requests.len(), 1);

        let update = VaultActionUpdateEvent::UpdateMembership {
            request: join_request,
            sender: UserDataMember { user_data: client_creds.user() },
            update: UserMembership::Member(UserDataMember { user_data: client_b_creds.user()}),
        };
        let event = VaultActionEvent::Update(update);
        let with_update_vault_request = actions.apply_event(event);
        assert_eq!(with_update_vault_request.requests.len(), 0);
        assert_eq!(with_update_vault_request.updates.len(), 1);

        Ok(())
    }
}
