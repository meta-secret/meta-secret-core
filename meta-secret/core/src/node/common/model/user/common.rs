use crate::node::common::model::device::common::{DeviceData, DeviceId};
use crate::node::common::model::vault::vault::VaultName;
use derive_more::From;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, From, Serialize, Deserialize)]
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

impl From<UserDataOutsider> for UserDataMember {
    fn from(outsider: UserDataOutsider) -> Self {
        UserDataMember {
            user_data: outsider.user_data,
        }
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
    pub fn pending(user_data: UserData) -> Self {
        Self {
            user_data,
            status: UserDataOutsiderStatus::Pending,
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::encoding::base64::Base64Text;
    use crate::crypto::keys::OpenBox;
    use crate::crypto::utils::U64IdUrlEnc;
    use crate::node::common::model::device::common::{DeviceData, DeviceId, DeviceName};
    use crate::node::common::model::vault::vault::VaultName;

    fn create_test_device_id() -> DeviceId {
        // Create U64IdUrlEnc from a string
        let text = Base64Text::from("test123".as_bytes());
        let id = U64IdUrlEnc { text };
        DeviceId(id)
    }

    fn create_test_device_data() -> DeviceData {
        let device_id = create_test_device_id();
        let device_name = DeviceName::from("test_device");

        // Create a minimal OpenBox with required fields
        let dsa_pk = crate::crypto::keys::DsaPk(Base64Text::from("test_dsa_pk".as_bytes()));
        let transport_text = Base64Text::from("test_transport".as_bytes());
        let transport_pk = crate::crypto::keys::TransportPk::from(transport_text);

        let open_box = OpenBox {
            dsa_pk,
            transport_pk,
        };

        DeviceData {
            device_id,
            device_name,
            keys: open_box,
        }
    }

    fn create_test_user_data() -> UserData {
        let vault_name = VaultName::from("test_vault");
        let device = create_test_device_data();

        UserData { vault_name, device }
    }

    #[test]
    fn test_user_data_methods() {
        let user_data = create_test_user_data();

        // Test vault_name method
        let vault_name = user_data.vault_name();
        assert_eq!(vault_name.0, "test_vault");

        // Test user_id method
        let user_id = user_data.user_id();
        assert_eq!(user_id.vault_name.0, "test_vault");
        // Compare device IDs directly since U64IdUrlEnc doesn't have as_u64 method
        assert_eq!(
            user_id.device_id.0.text.base64_str(),
            user_data.device.device_id.0.text.base64_str()
        );
    }

    #[test]
    fn test_user_membership_member() {
        let user_data = create_test_user_data();
        let member = UserDataMember {
            user_data: user_data.clone(),
        };
        let membership = UserMembership::Member(member.clone());

        // Test access methods
        assert!(matches!(membership, UserMembership::Member(_)));
        assert_eq!(membership.user_data(), user_data.clone());
        assert_eq!(
            membership.device_id().0.text.base64_str(),
            user_data.device.device_id.0.text.base64_str()
        );

        // Convert to WasmUserMembership
        let wasm_membership: WasmUserMembership = membership.into();
        assert!(wasm_membership.is_member());
        assert!(!wasm_membership.is_outsider());

        // Test as_member
        let member_data = wasm_membership.as_member();
        assert_eq!(member_data.user_data, user_data);
    }

    #[test]
    fn test_user_membership_outsider() {
        let user_data = create_test_user_data();
        let outsider = UserDataOutsider {
            user_data: user_data.clone(),
            status: UserDataOutsiderStatus::NonMember,
        };
        let membership = UserMembership::Outsider(outsider.clone());

        // Test methods
        assert!(matches!(membership, UserMembership::Outsider(_)));
        assert_eq!(membership.user_data(), user_data.clone());

        // Convert to WasmUserMembership
        let wasm_membership: WasmUserMembership = membership.into();
        assert!(wasm_membership.is_outsider());
        assert!(!wasm_membership.is_member());

        // Test as_outsider
        let outsider_data = wasm_membership.as_outsider();
        assert_eq!(outsider_data.user_data, user_data);
        assert_eq!(outsider_data.status, UserDataOutsiderStatus::NonMember);
    }

    #[test]
    fn test_user_data_outsider_methods() {
        let user_data = create_test_user_data();

        // Test non_member factory method
        let outsider = UserDataOutsider::non_member(user_data.clone());
        assert_eq!(outsider.user_data, user_data);
        assert_eq!(outsider.status, UserDataOutsiderStatus::NonMember);

        // Test is_non_member predicate
        assert!(outsider.is_non_member());
    }

    #[test]
    fn test_user_data_member_from_outsider() {
        let user_data = create_test_user_data();
        let outsider = UserDataOutsider::non_member(user_data.clone());

        // Convert from outsider to member
        let member: UserDataMember = outsider.into();
        assert_eq!(member.user_data, user_data);

        // Test user() method
        assert_eq!(member.user(), &user_data);
    }

    #[test]
    fn test_user_membership_user_data_member() {
        let user_data = create_test_user_data();

        // Test with Member
        let member = UserDataMember {
            user_data: user_data.clone(),
        };
        let membership = UserMembership::Member(member.clone());
        let result = membership.user_data_member();
        assert_eq!(result.user_data, user_data);

        // Test with Outsider
        let outsider = UserDataOutsider {
            user_data: user_data.clone(),
            status: UserDataOutsiderStatus::NonMember,
        };
        let membership = UserMembership::Outsider(outsider);
        let result = membership.user_data_member();
        assert_eq!(result.user_data, user_data);
    }
}
