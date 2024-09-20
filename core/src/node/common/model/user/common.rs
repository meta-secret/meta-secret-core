use crate::node::common::model::device::common::{DeviceData, DeviceId};
use crate::node::common::model::vault::VaultName;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserId {
    pub vault_name: VaultName,
    pub device_id: DeviceId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserData {
    pub vault_name: VaultName,
    pub device: DeviceData,
}

impl UserData {
    pub fn vault_name(&self) -> VaultName {
        self.vault_name.clone()
    }

    pub fn user_id(&self) -> UserId {
        UserId {
            vault_name: self.vault_name.clone(),
            device_id: self.device.id.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UserMembership {
    Outsider(UserDataOutsider),
    Member(UserDataMember),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDataMember(pub UserData);

impl UserDataMember {
    pub fn user(&self) -> &UserData {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDataOutsider {
    pub user_data: UserData,
    pub status: UserDataOutsiderStatus,
}

impl UserDataOutsider {
    pub fn non_member(user_data: UserData) -> Self {
        Self {
            user_data,
            status: UserDataOutsiderStatus::NonMember,
        }
    }

    pub fn is_non_member(&self) -> bool {
        self.status == UserDataOutsiderStatus::NonMember
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UserDataOutsiderStatus {
    /// Unknown status (the user is not a member of the vault), but the vault exists
    NonMember,
    Pending,
    Declined,
}

impl UserMembership {
    pub fn user_data(&self) -> UserData {
        match self {
            UserMembership::Outsider(UserDataOutsider { user_data, .. }) => user_data.clone(),
            UserMembership::Member(UserDataMember(member)) => member.clone(),
        }
    }

    pub fn device_id(&self) -> DeviceId {
        self.user_data().device.id.clone()
    }
}
