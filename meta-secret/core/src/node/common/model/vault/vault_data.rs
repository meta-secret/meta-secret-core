use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::user::common::{
    UserData, UserDataMember, UserDataOutsider, UserMembership, WasmUserMembership,
};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::events::vault::vault_log_event::{
    AddMetaPassEvent, VaultActionEvent, VaultActionEvents, VaultActionUpdateEvent,
};
use crate::secret::data_block::common::SharedSecretConfig;
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
            .map(|m| WasmUserMembership::from(m.clone()))
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

impl From<UserData> for VaultData {
    fn from(user_data: UserData) -> Self {
        let vault_name = user_data.vault_name.clone();
        let device_id = DeviceId::from(&user_data.device.keys);

        let member = UserMembership::Member(UserDataMember { user_data });

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
            user_data.device.device_id.eq(&device_id)
        } else {
            false
        }
    }

    pub fn membership(&self, for_user: UserData) -> UserMembership {
        let maybe_vault_user = self.users.get(&for_user.device.device_id);

        if let Some(membership) = maybe_vault_user {
            membership.clone()
        } else {
            UserMembership::Outsider(UserDataOutsider::non_member(for_user))
        }
    }

    pub fn find_user(&self, device_id: &DeviceId) -> Option<UserMembership> {
        self.users.get(device_id).cloned()
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
        let updates = self.events.updates.clone();

        for update in updates {
            self.events = self.events.apply_event(VaultActionEvent::Update(update));
        }

        self = self.complete();

        self
    }

    fn complete(mut self) -> Self {
        for curr_update in &self.events.updates {
            match curr_update {
                VaultActionUpdateEvent::UpdateMembership { sender, update, .. } => {
                    if self.vault.is_member(&sender.user().device.device_id) {
                        self.vault = self.vault.update_membership(update.clone());
                    }
                }
                VaultActionUpdateEvent::AddMetaPass(AddMetaPassEvent {
                    meta_pass_id,
                    sender,
                }) => {
                    if self.vault.is_member(&sender.user().device.device_id) {
                        self.vault = self.vault.add_secret(meta_pass_id.clone())
                    }
                }
            }
        }

        self.events = self.events.complete();
        self
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::user::common::{
        UserDataMember, UserDataOutsider, UserDataOutsiderStatus, UserMembership,
    };
    use crate::node::common::model::vault::vault_data::{VaultAggregate, VaultData};
    use crate::node::db::events::vault::vault_log_event::{
        JoinClusterEvent, VaultActionEvent, VaultActionEvents, VaultActionRequestEvent,
        VaultActionUpdateEvent,
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
        let client_b_membership = UserMembership::Outsider(UserDataOutsider {
            user_data: client_b_creds.user(),
            status: UserDataOutsiderStatus::Pending,
        });

        let vault_data = VaultData::from(client_creds.user());
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
        let vault_data = VaultData::from(client_creds.user());

        let join_request = JoinClusterEvent::from(fixture.state.user_creds.client_b.user());
        let join_request_event = {
            VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(join_request.clone()))
        };

        let update_membership = VaultActionUpdateEvent::UpdateMembership {
            request: join_request.clone(),
            sender: UserDataMember {
                user_data: client_creds.user(),
            },
            update: UserMembership::Member(UserDataMember {
                user_data: join_request.candidate.clone(),
            }),
        };

        let update_membership_event = VaultActionEvent::Update(update_membership);

        let events = VaultActionEvents::default()
            .apply_event(join_request_event)
            .apply_event(update_membership_event);

        let vault_aggregate = VaultAggregate::build_from(events, vault_data);
        assert_eq!(2, vault_aggregate.vault.users.len());

        Ok(())
    }
}
