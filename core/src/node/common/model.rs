use crate::models::MetaPasswordDoc;
use crate::node::common::model::device::DeviceCredentials;
use crate::node::common::model::vault::VaultData;

pub mod device {
    use crypto::utils::generate_uuid_b64_url_enc;

    use crate::crypto;
    use crate::crypto::keys::OpenBox;
    use crate::crypto::keys::SecretBox;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceData {
        id: DeviceId,
        name: String,
        pub keys: OpenBox,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceId(String);

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceCredentials {
        pub secret_box: SecretBox,
        pub device: DeviceData,
    }

    impl ToString for DeviceId {
        fn to_string(&self) -> String {
            self.0.clone()
        }
    }

    impl DeviceData {
        pub fn from(device_name: String, open_box: OpenBox) -> Self {
            Self {
                name: device_name,
                id: DeviceId::from(&open_box),
                keys: open_box,
            }
        }
    }

    impl From<&OpenBox> for DeviceId {
        fn from(open_box: &OpenBox) -> Self {
            let dsa_pk = open_box.dsa_pk.base64_text.clone();
            let id = generate_uuid_b64_url_enc(dsa_pk);
            Self(id)
        }
    }
}

pub mod user {
    use crate::node::common::model::device::DeviceData;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserData {
        pub vault_name: String,
        pub device: DeviceData,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserDataCandidate {
        pub data: UserData
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserDataMember {
        pub data: UserData
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserDataPending {
        pub data: UserData
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserDataDeclined {
        pub data: UserData
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum UserMembership {
        Candidate(UserDataCandidate),
        Member(UserDataMember),
        Pending(UserDataPending),
        Declined(UserDataDeclined)
    }
}

pub mod vault {
    use std::collections::HashMap;
    use crate::node::common::model::device::DeviceId;
    use crate::node::common::model::user::UserMembership;

    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct VaultData {
        pub vault_name: String,
        pub users: HashMap<DeviceId, UserMembership>,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[serde(tag = "__vault_ingo")]
    pub enum VaultInfo {
        /// Device is a member of a vault
        Member { vault: VaultData },
        /// Device is waiting to be added to a vault.
        Pending,
        /// Vault members declined to add a device into the vault.
        Declined,
        /// Vault not found
        NotFound,
        /// Device can't get any information about the vault, because its signature is not in members or pending list
        NotMember,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationState {
    pub device_creds: Option<DeviceCredentials>,
    pub vault: Option<VaultData>,
    pub meta_passwords: Vec<MetaPasswordDoc>,
    pub join_component: bool,
}
