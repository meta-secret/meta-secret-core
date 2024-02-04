use crate::node::common::model::device::{DeviceCredentials, DeviceData, DeviceId, DeviceName};
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCredentials {
    pub vault_name: VaultName,
    pub device_creds: DeviceCredentials,
}

impl UserCredentials {
    pub fn from(device_creds: DeviceCredentials, vault_name: VaultName) -> UserCredentials {
        UserCredentials {
            vault_name,
            device_creds,
        }
    }

    pub fn generate(device_name: DeviceName, vault_name: VaultName) -> UserCredentials {
        UserCredentials {
            vault_name,
            device_creds: DeviceCredentials::generate(device_name),
        }
    }

    pub fn device(&self) -> DeviceData {
        self.device_creds.device.clone()
    }

    pub fn user(&self) -> UserData {
        UserData {
            vault_name: self.vault_name.clone(),
            device: self.device(),
        }
    }

    pub fn user_id(&self) -> UserId {
        UserId {
            vault_name: self.vault_name.clone(),
            device_id: self.device().id.clone(),
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
    pub fn unknown(user_data: UserData) -> Self {
        Self {
            user_data,
            status: UserDataOutsiderStatus::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UserDataOutsiderStatus {
    /// Unknown status (the user is not a member of the vault)
    Unknown,
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
