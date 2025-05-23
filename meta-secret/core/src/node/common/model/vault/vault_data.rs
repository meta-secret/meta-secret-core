use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::user::common::{
    UserData, UserDataMember, UserDataOutsider, UserMembership, WasmUserMembership,
};
use crate::node::common::model::vault::vault::{VaultMember, VaultName, VaultStatus};
use crate::node::db::events::vault::vault_log_event::{
    AddMetaPassEvent, VaultActionEvents, VaultActionUpdateEvent,
};
use crate::secret::data_block::common::SharedSecretConfig;
use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultData {
    pub vault_name: VaultName,
    pub users: HashMap<DeviceId, UserMembership>,
    pub secrets: HashSet<MetaPasswordId>,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmVaultData(VaultData);

#[wasm_bindgen]
impl WasmVaultData {
    pub fn vault_name(&self) -> VaultName {
        self.0.vault_name.clone()
    }

    pub fn users(&self) -> Vec<WasmUserMembership> {
        self.0
            .users
            .values()
            .map(|user| WasmUserMembership::from(user.clone()))
            .collect()
    }

    pub fn members(&self) -> Vec<UserDataMember> {
        self.0.members()
    }

    pub fn outsiders(&self) -> Vec<UserDataOutsider> {
        self.0.outsiders()
    }

    pub fn secrets(&self) -> Vec<MetaPasswordId> {
        self.0.secrets.iter().cloned().collect()
    }
}

impl From<VaultData> for WasmVaultData {
    fn from(vault_data: VaultData) -> Self {
        WasmVaultData(vault_data)
    }
}

impl From<UserDataMember> for VaultData {
    fn from(member: UserDataMember) -> Self {
        let vault_name = member.user_data.vault_name();
        let device_id = DeviceId::from(&member.user_data.device.keys);

        let member = UserMembership::Member(member);

        let mut users = HashMap::new();
        users.insert(device_id, member);

        VaultData {
            vault_name,
            users,
            secrets: HashSet::new(),
        }
    }
}

impl VaultData {
    pub fn sss_cfg(&self) -> SharedSecretConfig {
        let members_num = self.members().len();
        SharedSecretConfig::calculate(members_num)
    }

    pub fn members(&self) -> Vec<UserDataMember> {
        let mut members: Vec<UserDataMember> = vec![];
        self.users.values().for_each(|membership| {
            if let UserMembership::Member(user_data_member) = membership {
                members.push(user_data_member.clone());
            }
        });

        members
    }

    pub fn outsiders(&self) -> Vec<UserDataOutsider> {
        let mut outsiders: Vec<UserDataOutsider> = vec![];
        self.users.values().for_each(|membership| {
            if let UserMembership::Outsider(outsider) = membership {
                outsiders.push(outsider.clone());
            }
        });

        outsiders
    }

    pub fn add_secret(mut self, meta_password_id: MetaPasswordId) -> Self {
        self.secrets.insert(meta_password_id);
        self
    }

    pub fn update_membership(mut self, membership: UserMembership) -> Self {
        self.users.insert(membership.device_id(), membership);
        self
    }

    pub fn is_not_member(&self, device_id: &DeviceId) -> bool {
        !self.is_member(device_id)
    }

    pub fn is_member(&self, device_id: &DeviceId) -> bool {
        let maybe_user = self.users.get(device_id);
        if let Some(UserMembership::Member(UserDataMember { user_data })) = maybe_user {
            user_data.device.device_id.eq(device_id)
        } else {
            false
        }
    }

    pub fn membership(&self, for_user: UserData) -> UserMembership {
        self.users
            .get(&for_user.device.device_id)
            .cloned()
            .unwrap_or_else(|| UserMembership::Outsider(UserDataOutsider::non_member(for_user)))
    }

    pub fn status(&self, user: UserData) -> VaultStatus {
        let membership = self.membership(user);
        VaultStatus::from(membership)
    }

    pub fn find_user(&self, device_id: &DeviceId) -> Option<&UserMembership> {
        self.users.get(device_id)
    }

    pub fn to_vault_member(self, member: UserDataMember) -> Result<VaultMember> {
        let is_member = self.is_member(&member.user_data.device.device_id.clone());

        if is_member {
            Ok(VaultMember {
                member,
                vault: self,
            })
        } else {
            bail!("User is not a member of this vault")
        }
    }
}

pub struct EmptyVaultState;

/// The documentation of aggregates in DDD/event source is
/// [here](meta-secret/docs/programming/event-sourcing-aggregate.md):
pub struct VaultAggregate {
    pub events: VaultActionEvents,
    pub vault: VaultData,
}

impl VaultAggregate {
    pub fn build_from(events: VaultActionEvents, vault: VaultData) -> Self {
        let current_state = Self { events, vault };
        current_state.synchronize()
    }

    /// Apply pending vault action events into the vault
    fn synchronize(mut self) -> Self {
        // Process each update in the events
        let updates = self.events.updates.clone();

        for update in updates {
            match &update {
                VaultActionUpdateEvent::UpdateMembership(membership) => {
                    if self
                        .vault
                        .is_member(&membership.sender.user().device.device_id)
                    {
                        self.vault = self.vault.update_membership(membership.update.clone());
                    }
                }
                VaultActionUpdateEvent::AddMetaPass(AddMetaPassEvent {
                    sender,
                    meta_pass_id,
                }) => {
                    if self.vault.is_member(&sender.user().device.device_id) {
                        self.vault = self.vault.add_secret(meta_pass_id.clone());
                    }
                }
                VaultActionUpdateEvent::AddToPending { candidate } => {
                    let pending =
                        UserMembership::Outsider(UserDataOutsider::pending(candidate.clone()));
                    self.vault = self.vault.update_membership(pending);
                }
            }
        }

        // After processing all updates, mark them as completed
        self.complete()
    }

    fn complete(mut self) -> Self {
        // This method is now just a fallback - all processing happens in synchronize
        self.events = self.events.complete();
        self
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use crate::node::common::model::user::common::{UserDataMember, UserMembership};
    use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
    use crate::node::common::model::vault::vault::VaultMember;
    use crate::node::common::model::vault::vault_data::VaultData;

    pub struct VaultDataFixture {
        pub full_membership: VaultData,
        pub client_membership: UserMembership,
        pub client_b_membership: UserMembership,
        pub vd_membership: UserMembership,

        pub client_vault_member: VaultMember,
        pub client_b_vault_member: VaultMember,
        pub vd_vault_member: VaultMember,
    }

    impl VaultDataFixture {
        pub fn from(creds: &UserCredentialsFixture) -> Self {
            let client_membership = {
                let client_creds = &creds.client;
                UserMembership::Member(UserDataMember {
                    user_data: client_creds.user(),
                })
            };

            let client_b_membership = {
                let client_b_creds = &creds.client_b;
                UserMembership::Member(UserDataMember {
                    user_data: client_b_creds.user(),
                })
            };

            let vd_membership = {
                let vd_creds = &creds.vd;
                UserMembership::Member(UserDataMember {
                    user_data: vd_creds.user(),
                })
            };

            let full_membership = VaultData::from(client_membership.user_data_member())
                .update_membership(vd_membership.clone())
                .update_membership(client_b_membership.clone());

            let client_vault_member = VaultMember {
                member: client_membership.user_data_member(),
                vault: full_membership.clone(),
            };

            let client_b_vault_member = VaultMember {
                member: client_b_membership.user_data_member(),
                vault: full_membership.clone(),
            };

            let vd_vault_member = VaultMember {
                member: vd_membership.user_data_member(),
                vault: full_membership.clone(),
            };

            Self {
                full_membership,
                client_membership,
                client_b_membership,
                vd_membership,
                client_vault_member,
                client_b_vault_member,
                vd_vault_member,
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::user::common::{
        UserDataMember, UserDataOutsider, UserMembership,
    };
    use crate::node::common::model::vault::vault_data::{VaultAggregate, VaultData};
    use crate::node::db::events::vault::vault_log_event::{
        AddMetaPassEvent, JoinClusterEvent, UpdateMembershipEvent, VaultActionEvent,
        VaultActionEvents, VaultActionRequestEvent, VaultActionUpdateEvent,
    };
    use anyhow::Result;

    #[test]
    fn test_sss_cfg() {
        let fixture = FixtureRegistry::empty();

        let client_creds = fixture.state.user_creds.client;

        let vd_creds = fixture.state.user_creds.vd;
        let vd_membership = UserMembership::Member(UserDataMember {
            user_data: vd_creds.user(),
        });

        let client_b_creds = fixture.state.user_creds.client_b;
        let client_b_membership =
            UserMembership::Outsider(UserDataOutsider::pending(client_b_creds.user()));

        let vault_data = VaultData::from(UserDataMember::from(client_creds.user()));
        let cfg = vault_data.sss_cfg();
        assert_eq!(cfg.threshold, 1);
        assert_eq!(cfg.number_of_shares, 1);

        let vault_data = vault_data.update_membership(vd_membership);
        let cfg = vault_data.sss_cfg();
        assert_eq!(cfg.threshold, 1);
        assert_eq!(cfg.number_of_shares, 2);

        let vault_data = vault_data.update_membership(client_b_membership);
        let cfg = vault_data.sss_cfg();
        assert_eq!(cfg.threshold, 1);
        assert_eq!(cfg.number_of_shares, 2);

        assert_eq!(2, vault_data.members().len());
        assert_eq!(1, vault_data.outsiders().len());
    }

    #[test]
    fn test_vault_aggregate() -> Result<()> {
        let fixture = FixtureRegistry::empty();
        let client_creds = fixture.state.user_creds.client;
        let vault_data = VaultData::from(UserDataMember::from(client_creds.user()));

        let join_request = JoinClusterEvent::from(fixture.state.user_creds.client_b.user());
        let join_request_event = {
            VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(join_request.clone()))
        };

        let update_membership = VaultActionUpdateEvent::UpdateMembership(UpdateMembershipEvent {
            request: join_request.clone(),
            sender: UserDataMember {
                user_data: client_creds.user(),
            },
            update: UserMembership::Member(UserDataMember {
                user_data: join_request.candidate.clone(),
            }),
        });

        let update_membership_event = VaultActionEvent::Update(update_membership);

        let events = VaultActionEvents::default()
            .apply_event(join_request_event)
            .apply_event(update_membership_event);

        let vault_aggregate = VaultAggregate::build_from(events, vault_data);
        assert_eq!(2, vault_aggregate.vault.users.len());

        Ok(())
    }

    #[test]
    fn test_vault_aggregate_complete_method() -> Result<()> {
        // Setup
        let fixture = FixtureRegistry::empty();
        let client_creds = fixture.state.user_creds.client;
        let vault_data = VaultData::from(UserDataMember::from(client_creds.user()));

        // Create empty events
        let events = VaultActionEvents::default();

        // Create aggregate
        let aggregate = VaultAggregate::build_from(events, vault_data);

        // Verify initial state
        assert_eq!(aggregate.events.updates.len(), 0);
        assert_eq!(aggregate.vault.members().len(), 1);

        Ok(())
    }

    #[test]
    fn test_vault_aggregate_with_add_meta_pass_event() -> Result<()> {
        // Setup
        let fixture = FixtureRegistry::empty();
        let client_creds = fixture.state.user_creds.client;
        let client_member = UserDataMember::from(client_creds.user());
        let vault_data = VaultData::from(client_member.clone());

        // Create meta password event
        let meta_pass_id = MetaPasswordId::build_from_str("Test Password");
        let add_meta_pass = AddMetaPassEvent {
            sender: client_member.clone(),
            meta_pass_id: meta_pass_id.clone(),
        };

        // First, create a request event
        let request_event = VaultActionRequestEvent::AddMetaPass(add_meta_pass.clone());

        // Then, create update event
        let update_event = VaultActionUpdateEvent::AddMetaPass(add_meta_pass);

        // Create events with both the request and the update
        let events = VaultActionEvents::default()
            .request(request_event)
            .apply(update_event);

        // Create aggregate and process
        let aggregate = VaultAggregate::build_from(events, vault_data);

        // Verify the meta pass was added
        assert!(aggregate.vault.secrets.contains(&meta_pass_id));
        assert_eq!(aggregate.events.updates.len(), 0); // Events should be processed

        Ok(())
    }

    #[test]
    fn test_vault_aggregate_sender_not_member() -> Result<()> {
        // Setup
        let fixture = FixtureRegistry::empty();
        let client_creds = fixture.state.user_creds.client;
        let client_b_creds = fixture.state.user_creds.client_b;
        let vault_data = VaultData::from(UserDataMember::from(client_creds.user()));

        // Create event with non-member sender
        let meta_pass_id = MetaPasswordId::build_from_str("Another Test Password");
        let non_member_sender = UserDataMember {
            user_data: client_b_creds.user(),
        };
        let add_meta_pass = AddMetaPassEvent {
            sender: non_member_sender,
            meta_pass_id: meta_pass_id.clone(),
        };

        // Create update event
        let update_event = VaultActionUpdateEvent::AddMetaPass(add_meta_pass);

        // Create events with the update
        let events = VaultActionEvents::default().apply(update_event);

        // Create aggregate and process
        let aggregate = VaultAggregate::build_from(events, vault_data);

        // Verify the meta pass was NOT added (sender not a member)
        assert!(!aggregate.vault.secrets.contains(&meta_pass_id));
        assert_eq!(aggregate.events.updates.len(), 0); // Events should be processed but not applied

        Ok(())
    }

    #[test]
    fn test_vault_aggregate_add_member_update() -> Result<()> {
        // Setup
        let fixture = FixtureRegistry::empty();
        let client_creds = fixture.state.user_creds.client;
        let client_b_creds = fixture.state.user_creds.client_b;
        let vault_data = VaultData::from(UserDataMember::from(client_creds.user()));

        // Create join request
        let join_request = JoinClusterEvent::from(client_b_creds.user());

        // Create request event
        let request_event = VaultActionRequestEvent::JoinCluster(join_request.clone());

        // Create member update
        let update_membership = VaultActionUpdateEvent::UpdateMembership(UpdateMembershipEvent {
            request: join_request.clone(),
            sender: UserDataMember::from(client_creds.user()), // Valid member as sender
            update: UserMembership::Member(UserDataMember::from(client_b_creds.user())),
        });

        // Create events with the request and update
        let events = VaultActionEvents::default()
            .request(request_event)
            .apply(update_membership);

        // Create aggregate and process
        let aggregate = VaultAggregate::build_from(events, vault_data);

        // Verify the new member was added
        assert_eq!(aggregate.vault.members().len(), 2);
        assert!(
            aggregate
                .vault
                .is_member(&client_b_creds.user().device.device_id)
        );
        assert_eq!(aggregate.events.updates.len(), 0); // Events should be processed

        Ok(())
    }

    #[test]
    fn test_vault_aggregate_multiple_events() -> Result<()> {
        // Setup
        let fixture = FixtureRegistry::empty();
        let client_creds = fixture.state.user_creds.client;
        let client_b_creds = fixture.state.user_creds.client_b;
        let vd_creds = fixture.state.user_creds.vd;
        let vault_data = VaultData::from(UserDataMember::from(client_creds.user()));

        // Create first join request and member update
        let join_request_b = JoinClusterEvent::from(client_b_creds.user());
        let request_event_b = VaultActionRequestEvent::JoinCluster(join_request_b.clone());
        let update_membership_b = VaultActionUpdateEvent::UpdateMembership(UpdateMembershipEvent {
            request: join_request_b.clone(),
            sender: UserDataMember::from(client_creds.user()),
            update: UserMembership::Member(UserDataMember::from(client_b_creds.user())),
        });

        // Create second join request and member update
        let join_request_vd = JoinClusterEvent::from(vd_creds.user());
        let request_event_vd = VaultActionRequestEvent::JoinCluster(join_request_vd.clone());
        let update_membership_vd =
            VaultActionUpdateEvent::UpdateMembership(UpdateMembershipEvent {
                request: join_request_vd.clone(),
                sender: UserDataMember::from(client_creds.user()),
                update: UserMembership::Member(UserDataMember::from(vd_creds.user())),
            });

        // Create events with both requests and updates
        let events = VaultActionEvents::default()
            .request(request_event_b)
            .apply(update_membership_b)
            .request(request_event_vd)
            .apply(update_membership_vd);

        // Create aggregate and process
        let aggregate = VaultAggregate::build_from(events, vault_data);

        // Verify both members were added
        assert_eq!(aggregate.vault.members().len(), 3);
        assert!(
            aggregate
                .vault
                .is_member(&client_b_creds.user().device.device_id)
        );
        assert!(aggregate.vault.is_member(&vd_creds.user().device.device_id));
        assert_eq!(aggregate.events.updates.len(), 0);

        Ok(())
    }
}
