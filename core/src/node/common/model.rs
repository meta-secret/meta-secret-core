use crate::node::common::model::crypto::EncryptedMessage;
use crate::node::common::model::device::DeviceCredentials;
use crate::node::common::model::user::UserId;
use crate::node::common::model::vault::VaultData;

pub mod device {
    use crypto::utils::generate_uuid_b64_url_enc;

    use crate::crypto;
    use crate::crypto::keys::{KeyManager, OpenBox};
    use crate::crypto::keys::SecretBox;

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceId(String);

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceName(String);

    impl From<String> for DeviceName {
        fn from(device_name: String) -> Self {
            DeviceName(device_name)
        }
    }

    impl From<&str> for DeviceName {
        fn from(device_name: &str) -> Self {
            DeviceName(String::from(device_name))
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceData {
        id: DeviceId,
        name: DeviceName,
        pub keys: OpenBox,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceCredentials {
        pub secret_box: SecretBox,
        pub device: DeviceData,
    }

    /// Contains full information about device (private keys and device id)
    impl DeviceCredentials {
        pub fn generate(device_name: DeviceName) -> DeviceCredentials {
            let secret_box = KeyManager::generate_secret_box();
            let device = DeviceData::from(device_name, OpenBox::from(&secret_box));
            DeviceCredentials { secret_box, device }
        }
    }

    impl ToString for DeviceId {
        fn to_string(&self) -> String {
            self.0.clone()
        }
    }

    /// Contains only public information about device
    impl DeviceData {
        pub fn from(device_name: DeviceName, open_box: OpenBox) -> Self {
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
    use crate::node::common::model::device::{DeviceCredentials, DeviceData, DeviceId, DeviceName};
    use crate::node::common::model::vault::VaultName;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserId {
        vault_name: VaultName,
        device_id: DeviceId
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserData {
        pub vault_name: VaultName,
        pub device: DeviceData,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserCredentials {
        pub vault_name: VaultName,
        pub device_creds: DeviceCredentials,
    }

    impl UserCredentials {

        pub fn from(device_creds: DeviceCredentials, vault_name: VaultName) -> UserCredentials {
            UserCredentials { vault_name, device_creds }
        }

        pub fn generate(device_name: DeviceName, vault_name: VaultName) -> UserCredentials {
            UserCredentials {
                vault_name,
                device_creds: DeviceCredentials::generate(device_name)
            }
        }
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
    use std::collections::{HashMap, HashSet};
    use crate::node::common::model::device::DeviceId;
    use crate::node::common::model::MetaPasswordId;
    use crate::node::common::model::user::{UserData, UserMembership};

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

    impl ToString for VaultName {
        fn to_string(&self) -> String {
            self.0.clone()
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct VaultData {
        pub vault_name: VaultName,
        pub users: HashMap<DeviceId, UserMembership>,
        pub secrets: HashSet<MetaPasswordId>
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[serde(tag = "__vault_ingo")]
    pub enum VaultInfo {
        /// Device is a member of a vault
        Member { vault: VaultData },
        /// Device is waiting to be added to a vault.
        Pending { user: UserData },
        /// Vault members declined to add a device into the vault.
        Declined { user: UserData },
        /// Vault not found
        NotFound,
        /// Device can't get any information about the vault, because its signature is not in members or pending list
        NotMember,
    }
}

mod crypto {
    use crate::crypto::encoding::base64::Base64Text;
    use crate::node::common::model::user::UserData;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AeadAuthData {
        pub associated_data: String,
        pub channel: CommunicationChannel,
        pub nonce: Base64Text,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AeadCipherText {
        pub msg: Base64Text,
        pub auth_data: AeadAuthData,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AeadPlainText {
        pub msg: Base64Text,
        pub auth_data: AeadAuthData,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CommunicationChannel {
        pub sender: Base64Text,
        pub receiver: Base64Text,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct EncryptedMessage {
        pub receiver: UserData,
        pub encrypted_text: AeadCipherText,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretDistributionType {
    Split,
    Recover,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretDistributionDocData {
    pub distribution_type: SecretDistributionType,
    pub meta_password: MetaPasswordRequest,
    pub secret_message: EncryptedMessage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RegistrationStatus {
    Registered,
    AlreadyExists,
}

impl ToString for RegistrationStatus {
    fn to_string(&self) -> String {
        match self {
            Self::Registered => String::from("Registered"),
            Self::AlreadyExists => String::from("AlreadyExists"),
        }
    }
}

impl Default for RegistrationStatus {
    fn default() -> RegistrationStatus {
        Self::Registered
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordRecoveryRequest {
    pub id: MetaPasswordId,
    pub consumer: UserId,
    pub provider: UserId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaPasswordRequest {
    pub user_id: UserId,
    pub meta_password: MetaPasswordData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaPasswordId {
    /// SHA256 hash of a salt
    pub id: String,
    /// Random String up to 30 characters, must be unique
    pub salt: String,
    /// Human readable name given to the password
    pub name: String,
}

/*#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaPasswordData {
    pub id: MetaPasswordId,
    pub vault: VaultData,
}*/

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationState {
    pub device_creds: Option<DeviceCredentials>,
    pub vault: Option<VaultData>,
    pub meta_passwords: Vec<MetaPasswordData>,
    pub join_component: bool,
}
