use crate::node::common::model::device::common::{DeviceData, DeviceId};
use crate::node::common::model::vault::vault::VaultName;
use derive_more::From;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct UserId {
    pub vault_name: VaultName,
    pub device_id: DeviceId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
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
            device_id: self.device.device_id.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UserMembership {
    Outsider(UserDataOutsider),
    Member(UserDataMember),
}

#[wasm_bindgen]
pub struct WasmUserMembership(UserMembership);

#[wasm_bindgen]
impl WasmUserMembership {
    pub fn is_outsider(&self) -> bool {
        matches!(self.0, UserMembership::Outsider(_))
    }

    pub fn is_member(&self) -> bool {
        matches!(self.0, UserMembership::Member(_))
    }

    pub fn as_outsider(&self) -> UserDataOutsider {
        match &self.0 {
            UserMembership::Outsider(data) => data.clone(),
            _ => panic!("user membership has to be outsider"),
        }
    }

    pub fn as_member(&self) -> UserDataMember {
        match &self.0 {
            UserMembership::Member(data) => data.clone(),
            _ => panic!("user membership has to be member"),
        }
    }

    pub fn user_data(&self) -> UserData {
        self.0.user_data()
    }
}

impl From<UserMembership> for WasmUserMembership {
    fn from(membership: UserMembership) -> Self {
        Self(membership)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct UserDataMember {
    pub user_data: UserData,
}

impl UserDataMember {
    pub fn user(&self) -> &UserData {
        &self.user_data
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub enum UserDataOutsiderStatus {
    /// Unknown status (the user is not a member of the vault), but the vault exists
    NonMember,
    Pending,
    Declined,
}

impl UserMembership {
    pub fn user_data_member(&self) -> UserDataMember {
        UserDataMember {
            user_data: self.user_data(),
        }
    }

    pub fn user_data(&self) -> UserData {
        match self {
            UserMembership::Outsider(UserDataOutsider { user_data, .. }) => user_data.clone(),
            UserMembership::Member(UserDataMember { user_data }) => user_data.clone(),
        }
    }

    pub fn device_id(&self) -> DeviceId {
        self.user_data().device.device_id.clone()
    }
}
