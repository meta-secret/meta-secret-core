use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::secret::ClaimId;
use crate::node::common::model::user::common::{
    UserData, UserDataMember, UserDataOutsiderStatus, UserMembership,
};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::descriptors::vault_descriptor::VaultLogDescriptor;
use crate::node::db::events::error::LogEventCastError;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ArtifactId;
use anyhow::{Result, anyhow, bail};
use derive_more::From;
use std::collections::HashSet;
use std::fmt::Display;
use tracing::info;
use wasm_bindgen::prelude::wasm_bindgen;

/// VaultLog keeps incoming events in order, the log is a queue for incoming messages and used to
/// recreate the vault state from events (event sourcing)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultLogObject(pub KvLogEvent<VaultActionEvents>);

impl VaultLogObject {
    pub fn create(owner: UserDataMember) -> Self {
        Self(KvLogEvent {
            key: KvKey::from(VaultLogDescriptor::from(owner.user_data.vault_name())),
            value: VaultActionEvents::default(),
        })
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
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}

impl ObjIdExtractor for VaultLogObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}

/// Represents all pending events to apply to the VaultObject
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultActionEvents {
    pub requests: HashSet<VaultActionRequestEvent>,
    pub updates: HashSet<VaultActionUpdateEvent>,
    /// Compact terminal JOIN decisions retained after transient updates are
    /// cleared, so an exact request can receive its result on reconnect.
    #[serde(default)]
    pub join_results: HashSet<JoinResultRecord>,
    #[serde(default)]
    pub deletion_results: HashSet<DeleteSecretDecision>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinResultOutcome {
    Accepted,
    Declined,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinResultRecord {
    pub request: JoinClusterEvent,
    pub outcome: JoinResultOutcome,
}

impl VaultActionEvents {
    pub fn retire_invalid_deletions(mut self, vault: &VaultData) -> Self {
        let stale: Vec<_> = self
            .requests
            .iter()
            .filter_map(|event| match event {
                VaultActionRequestEvent::DeleteSecret(request)
                    if vault.deletion_results.contains_key(&request.request_id)
                        || vault.ensure_can_delete_secret(request, None).is_err() =>
                {
                    Some(request.clone())
                }
                _ => None,
            })
            .collect();
        for request in stale {
            self.requests
                .remove(&VaultActionRequestEvent::DeleteSecret(request.clone()));
            self.deletion_results.insert(
                vault
                    .deletion_results
                    .get(&request.request_id)
                    .cloned()
                    .unwrap_or_else(|| DeleteSecretDecision {
                        sender: request.sender.clone(),
                        request,
                        accepted: false,
                    }),
            );
        }
        self
    }
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
                self = self.request(request);
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

    pub fn request(mut self, request: VaultActionRequestEvent) -> Self {
        self.requests.insert(request);
        self
    }

    pub fn apply(mut self, upd_event: VaultActionUpdateEvent) -> Self {
        match &upd_event {
            VaultActionUpdateEvent::UpdateMembership(update) => {
                let request = VaultActionRequestEvent::JoinCluster(update.request.clone());
                let removed = self.requests.remove(&request);
                // if corresponding request exists we can apply the update
                if removed {
                    let outcome = match &update.update {
                        UserMembership::Member(member)
                            if member.user_data == update.request.candidate =>
                        {
                            Some(JoinResultOutcome::Accepted)
                        }
                        UserMembership::Outsider(outsider)
                            if outsider.user_data == update.request.candidate
                                && outsider.status == UserDataOutsiderStatus::Declined =>
                        {
                            Some(JoinResultOutcome::Declined)
                        }
                        _ => None,
                    };
                    if let Some(outcome) = outcome {
                        self.join_results.insert(JoinResultRecord {
                            request: update.request.clone(),
                            outcome,
                        });
                    }
                    self.updates.insert(upd_event);
                } else {
                    info!(
                        "Corresponding request not found: {:?}, update won't be applied",
                        request
                    );
                }
            }
            VaultActionUpdateEvent::RemoveMember(update) => {
                let request = VaultActionRequestEvent::RemoveMember(update.request.clone());
                let removed = self.requests.remove(&request);
                if removed {
                    self.updates.insert(upd_event);
                } else {
                    info!(
                        "Corresponding remove-member request not found: {:?}, update won't be applied",
                        request
                    );
                }
            }
            VaultActionUpdateEvent::DeleteSecret(update) => {
                if self.requests.remove(&VaultActionRequestEvent::DeleteSecret(
                    update.request.clone(),
                )) {
                    self.deletion_results.insert(update.clone());
                    self.updates.insert(upd_event);
                }
            }
            VaultActionUpdateEvent::AddMetaPass(event) => {
                let request = VaultActionRequestEvent::AddMetaPass(event.clone());
                let removed = self.requests.remove(&request);
                // if corresponding request exists we can apply the update
                if removed {
                    self.updates.insert(upd_event);
                } else {
                    info!(
                        "Corresponding request not found: {:?}, update won't be applied",
                        request
                    );
                }
            }
            VaultActionUpdateEvent::AddToPending { .. } => {
                self.updates.insert(upd_event);
            }
        };

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
#[allow(clippy::large_enum_variant)]
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
    RemoveMember(RemoveMemberRequestEvent),
    AddMetaPass(AddMetaPassEvent),
    DeleteSecret(DeleteSecretRequest),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSecretRequest {
    pub request_id: String,
    pub sender: UserDataMember,
    pub pass_id: MetaPasswordId,
    pub members: Vec<DeviceId>,
}

impl DeleteSecretRequest {
    pub fn new(sender: UserDataMember, pass_id: MetaPasswordId, vault: &VaultData) -> Self {
        let mut members: Vec<_> = vault
            .members()
            .into_iter()
            .map(|m| m.user_data.device.device_id)
            .collect();
        members.sort_by_key(ToString::to_string);
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            sender,
            pass_id,
            members,
        }
    }

    pub fn requires_approval(&self) -> bool {
        self.members.len() >= 3
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSecretDecision {
    pub request: DeleteSecretRequest,
    pub sender: UserDataMember,
    pub accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVaultEvent {
    pub owner: UserDataMember,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct JoinClusterEvent {
    pub candidate: UserData,
    /// Core-generated identifier for this JOIN attempt. Empty only when
    /// deserializing historical events written before request-scoped SSE.
    #[serde(default)]
    pub request_id: String,
}

impl From<UserData> for JoinClusterEvent {
    fn from(candidate: UserData) -> Self {
        Self {
            candidate,
            request_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMemberRequestEvent {
    pub sender: UserDataMember,
    pub target: UserDataMember,
    /// The initiator's encrypted local shares, addressed to the member that
    /// will approve the removal. The server only relays ciphertext.
    #[serde(default)]
    pub share_transfers: Vec<RemoveMemberShareTransfer>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMetaPassEvent {
    pub sender: UserDataMember,
    pub meta_pass_id: MetaPasswordId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMembershipEvent {
    pub request: JoinClusterEvent,
    pub sender: UserDataMember,
    pub update: UserMembership,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoveMemberDecision {
    /// The approver accepted the request and the target leaves the Vault.
    #[default]
    Accepted,
    /// The approver rejected the request. Membership and Key Shares stay unchanged.
    Declined,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMemberEvent {
    pub request: RemoveMemberRequestEvent,
    pub sender: UserDataMember,
    pub target: UserDataMember,
    pub new_status: UserMembership,
    /// Missing in historical events, which were all approvals.
    #[serde(default)]
    pub decision: RemoveMemberDecision,
    /// Encrypted Key Shares sent by the approver to the initiator so the
    /// initiator can complete 3→2 resharing. Values are ciphertext JSON only;
    /// the server cannot decrypt them.
    #[serde(default)]
    pub share_transfers: Vec<RemoveMemberShareTransfer>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMemberShareTransfer {
    pub pass_id: MetaPasswordId,
    pub receiver: DeviceId,
    /// Split claim generation that owns the encrypted input share. Optional
    /// only for decoding removal events written before generation binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<ClaimId>,
    pub encrypted_share: String,
}

impl VaultActionRequestEvent {
    pub fn name(&self) -> String {
        let name = match self {
            VaultActionRequestEvent::JoinCluster { .. } => "JoinRequest",
            VaultActionRequestEvent::RemoveMember { .. } => "RemoveMemberRequest",
            VaultActionRequestEvent::AddMetaPass { .. } => "AddMetaPasswordRequest",
            VaultActionRequestEvent::DeleteSecret { .. } => "DeleteSecretRequest",
        };

        String::from(name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::large_enum_variant)]
pub enum VaultActionUpdateEvent {
    /// There is no corresponding request for this event (server prematurely adds candidate to pending list)
    AddToPending {
        candidate: UserData,
    },
    /// When the device becomes a member of the vault, it can change membership of other members
    UpdateMembership(UpdateMembershipEvent),
    /// A current member removes another member after the matching request.
    RemoveMember(RemoveMemberEvent),
    /// A member can add a new meta password into the vault
    AddMetaPass(AddMetaPassEvent),
    DeleteSecret(DeleteSecretDecision),
}

impl VaultActionUpdateEvent {
    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultActionUpdateEvent::UpdateMembership(update) => {
                update.update.user_data().vault_name()
            }
            VaultActionUpdateEvent::RemoveMember(update) => update.target.user_data.vault_name(),
            VaultActionUpdateEvent::AddMetaPass(AddMetaPassEvent { sender, .. }) => {
                sender.user_data.vault_name()
            }
            VaultActionUpdateEvent::AddToPending { candidate } => candidate.vault_name(),
            VaultActionUpdateEvent::DeleteSecret(event) => {
                event.request.sender.user_data.vault_name()
            }
        }
    }
}

impl VaultActionRequestEvent {
    pub fn vault_name(&self) -> VaultName {
        match self {
            VaultActionRequestEvent::JoinCluster(request) => request.candidate.vault_name(),
            VaultActionRequestEvent::RemoveMember(request) => request.sender.user_data.vault_name(),
            VaultActionRequestEvent::AddMetaPass(request) => request.sender.user_data.vault_name(),
            VaultActionRequestEvent::DeleteSecret(request) => request.sender.user_data.vault_name(),
        }
    }
}

impl VaultActionUpdateEvent {
    pub fn name(&self) -> String {
        let name = match self {
            VaultActionUpdateEvent::UpdateMembership { .. } => "UpdateMembership",
            VaultActionUpdateEvent::RemoveMember { .. } => "RemoveMember",
            VaultActionUpdateEvent::AddMetaPass { .. } => "AddMetaPassword",
            VaultActionUpdateEvent::AddToPending { .. } => "AddToPending",
            VaultActionUpdateEvent::DeleteSecret { .. } => "DeleteSecret",
        };

        name.to_string()
    }
}

impl VaultActionEvent {
    fn name(&self) -> String {
        match self {
            VaultActionEvent::Request(request) => request.name(),
            VaultActionEvent::Update(update) => update.name(),
            VaultActionEvent::Init(_) => "CreateVaultInit".to_string(),
        }
    }
}

impl Display for VaultActionEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl VaultActionEvent {
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
            VaultActionEvent::Request(request) => {
                let user = match request {
                    VaultActionRequestEvent::JoinCluster(event) => &event.candidate,
                    VaultActionRequestEvent::RemoveMember(event) => &event.sender.user_data,
                    VaultActionRequestEvent::AddMetaPass(event) => &event.sender.user_data,
                    VaultActionRequestEvent::DeleteSecret(event) => &event.sender.user_data,
                };
                user.vault_name()
            }
            VaultActionEvent::Update(update) => update.vault_name(),
            VaultActionEvent::Init(VaultActionInitEvent::CreateVault(event)) => {
                event.owner.user_data.vault_name()
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::user::common::{UserDataMember, UserMembership};
    use crate::node::db::events::vault::vault_log_event::UpdateMembershipEvent;
    use crate::node::db::events::vault::vault_log_event::{
        JoinClusterEvent, VaultActionEvent, VaultActionEvents, VaultActionRequestEvent,
        VaultActionUpdateEvent,
    };
    use anyhow::Result;

    #[test]
    fn test_create_vault_action_progression() -> Result<()> {
        let fixture = FixtureRegistry::empty();
        let client_creds = fixture.state.user_creds.client;
        let client_b_creds = fixture.state.user_creds.client_b;

        let join_request = JoinClusterEvent {
            candidate: client_creds.user(),
            request_id: "request-test".into(),
        };
        let event =
            VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(join_request.clone()));

        let actions = VaultActionEvents::default().apply_event(event);
        assert_eq!(actions.requests.len(), 1);

        let update = VaultActionUpdateEvent::UpdateMembership(UpdateMembershipEvent {
            request: join_request,
            sender: UserDataMember {
                user_data: client_creds.user(),
            },
            update: UserMembership::Member(UserDataMember {
                user_data: client_b_creds.user(),
            }),
        });
        let event = VaultActionEvent::Update(update);
        let with_update_vault_request = actions.apply_event(event);
        assert_eq!(with_update_vault_request.requests.len(), 0);
        assert_eq!(with_update_vault_request.updates.len(), 1);

        Ok(())
    }

    #[test]
    fn terminal_join_result_survives_completion_and_legacy_deserialization() {
        let fixture = FixtureRegistry::empty();
        let candidate = fixture.state.user_creds.vd.user();
        let sender = UserDataMember::from(fixture.state.user_creds.client.user());
        let request = JoinClusterEvent {
            candidate: candidate.clone(),
            request_id: "terminal-record-test".into(),
        };
        let pending = VaultActionEvents::default()
            .request(VaultActionRequestEvent::JoinCluster(request.clone()));
        let terminal = pending.apply(VaultActionUpdateEvent::UpdateMembership(
            UpdateMembershipEvent {
                request: request.clone(),
                sender,
                update: UserMembership::Member(UserDataMember {
                    user_data: candidate,
                }),
            },
        ));

        assert_eq!(terminal.join_results.len(), 1);
        let completed = terminal.complete();
        assert!(completed.updates.is_empty());
        assert_eq!(completed.join_results.len(), 1);

        let decoded: VaultActionEvents =
            serde_json::from_str(r#"{"requests":[],"updates":[]}"#).unwrap();
        assert!(decoded.join_results.is_empty());
    }
}
