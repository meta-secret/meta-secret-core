use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::user::common::{
    UserData, UserDataMember, UserDataOutsider, UserMembership, WasmUserMembership,
};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::events::vault::vault_log_event::{AddMetaPassEvent, VaultActionEvents, VaultActionUpdateEvent};
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

    pub fn add_secret(self, meta_password_id: MetaPasswordId) -> Self {
        let mut secrets: HashSet<_> = self.secrets.iter().cloned().collect();
        secrets.insert(meta_password_id);

        Self {
            vault_name: self.vault_name,
            users: self.users,
            secrets,
        }
    }

    pub fn update_membership(self, membership: UserMembership) -> Self {
        let mut new_vault = self;
        new_vault.users.insert(membership.device_id(), membership);
        new_vault
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

    /// Take all vault action events and update vault with those events, then return updated vault
    pub fn update(self, updates: &VaultActionEvents) -> VaultData {
        let mut new_vault = self;

        let upd_events = updates.get_update_events();
        for upd_event in upd_events {
            match upd_event {
                VaultActionUpdateEvent::CreateVault { .. } => {
                    //ignore
                }
                VaultActionUpdateEvent::UpdateMembership { sender, update, .. } => {
                    if new_vault.is_member(&sender.user().device.device_id) {
                        new_vault = new_vault.update_membership(update);
                    }
                }
                VaultActionUpdateEvent::AddMetaPass(AddMetaPassEvent {
                    meta_pass_id,
                    sender,
                }) => {
                    if new_vault.is_member(&sender.user().device.device_id) {
                        new_vault = new_vault.add_secret(meta_pass_id)
                    }
                }
            }
        }

        new_vault
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::user::common::{
        UserDataMember, UserDataOutsider, UserDataOutsiderStatus, UserMembership,
    };
    use crate::node::common::model::vault::vault_data::VaultData;

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
}
