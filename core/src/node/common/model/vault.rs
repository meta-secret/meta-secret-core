use std::collections::{HashMap, HashSet};
use std::fmt::Display;

use crate::node::common::model::device::DeviceId;
use crate::node::common::model::MetaPasswordId;
use crate::node::common::model::user::{UserData, UserDataMember, UserDataOutsider, UserMembership};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::vault_event::VaultObject;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultName(pub String);

impl From<String> for VaultName {
    fn from(vault_name: String) -> Self {
        Self(vault_name)
    }
}

impl From<&str> for VaultName {
    fn from(vault_name: &str) -> Self {
        VaultName::from(String::from(vault_name))
    }
}

impl Display for VaultName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultData {
    pub vault_name: VaultName,
    pub users: HashMap<DeviceId, UserMembership>,
    pub secrets: HashSet<MetaPasswordId>,
}

impl From<VaultName> for VaultData {
    fn from(vault_name: VaultName) -> Self {
        VaultData {
            vault_name,
            users: HashMap::new(),
            secrets: HashSet::new(),
        }
    }
}

impl VaultData {
    pub fn members(&self) -> Vec<UserDataMember> {
        let mut members: Vec<UserDataMember> = vec![];
        self.users.values().for_each(|membership| {
            if let UserMembership::Member(user_data_member) = membership {
                members.push(user_data_member.clone());
            }
        });

        members
    }

    pub fn add_secret(&mut self, meta_password_id: MetaPasswordId) {
        self.secrets.insert(meta_password_id);
    }

    pub fn update_membership(&mut self, membership: UserMembership) {
        self.users.insert(membership.device_id(), membership);
    }

    pub fn is_member(&self, device_id: &DeviceId) -> bool {
        let maybe_user = self.users.get(device_id);
        if let Some(UserMembership::Member(UserDataMember(user_data))) = maybe_user {
            user_data.device.id == device_id.clone()
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultStatus {
    Outsider(UserDataOutsider),
    Member(VaultData),
}

impl VaultStatus {
    pub fn try_from(vault_event: GenericKvLogEvent, user: UserData) -> anyhow::Result<Self> {
        let vault_obj = VaultObject::try_from(vault_event)?;
        match vault_obj {
            VaultObject::Unit { .. } => {
                Ok(VaultStatus::Outsider(UserDataOutsider::unknown(user)))
            }
            VaultObject::Genesis { .. } => {
                Ok(VaultStatus::Outsider(UserDataOutsider::unknown(user)))
            }
            VaultObject::Vault(event) => {
                Ok(VaultStatus::Member(event.value))
            }
        }
    }
}

impl VaultStatus {
    pub fn unknown(user: UserData) -> Self {
        VaultStatus::Outsider(UserDataOutsider::unknown(user))
    }
}
