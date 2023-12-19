use std::fmt::Display;

use rand::Rng;

use crate::node::common::model::device::DeviceData;
use crate::node::common::model::secret::MetaPasswordId;
use crate::node::common::model::vault::VaultStatus;

pub mod device;
pub mod user;
pub mod vault;

pub mod crypto {
    use crate::crypto::encoding::base64::Base64Text;
    use crate::node::common::model::device::DeviceLink;

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
    pub enum EncryptedMessage {
        /// There is only one type of encrypted message for now, which is encrypted share of a secret,
        /// and that particular type of message has a device link
        /// and it used to figure out which vault the message belongs to
        CipherShare {
            device_link: DeviceLink,
            share: AeadCipherText
        }
    }

    impl EncryptedMessage {
        pub fn device_link(&self) -> DeviceLink {
            match self {
                EncryptedMessage::CipherShare { device_link, .. } => device_link.clone()
            }
        }
    }
}

pub mod secret {
    use rand::distributions::Alphanumeric;
    use rand::Rng;

    use crate::crypto::utils;
    use crate::node::common::model::crypto::EncryptedMessage;
    use crate::node::common::model::device::DeviceLink;
    use crate::node::common::model::vault::VaultName;

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

    const SALT_LENGTH: usize = 8;

    impl MetaPasswordId {
        pub fn generate(name: String) -> Self {
            let salt: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(SALT_LENGTH)
                .map(char::from)
                .collect();
            MetaPasswordId::build(name, salt)
        }

        pub fn build(name: String, salt: String) -> Self {
            let mut id_str = name.clone();
            id_str.push('-');
            id_str.push_str(salt.as_str());

            Self {
                id: utils::generate_uuid_b64_url_enc(id_str),
                salt,
                name,
            }
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
    pub struct SecretDistributionData {
        pub distribution_type: SecretDistributionType,
        pub vault_name: VaultName,
        pub meta_password_id: MetaPasswordId,
        pub secret_message: EncryptedMessage,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PasswordRecoveryRequest {
        pub id: MetaPasswordId,
        pub device_link: DeviceLink
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RegistrationStatus {
    Registered,
    AlreadyExists,
}

impl Display for RegistrationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Self::Registered => String::from("Registered"),
            Self::AlreadyExists => String::from("AlreadyExists"),
        };
        write!(f, "{}", str)
    }
}

impl Default for RegistrationStatus {
    fn default() -> RegistrationStatus {
        Self::Registered
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationState {
    pub device: Option<DeviceData>,
    pub vault: Option<VaultStatus>,
    pub join_component: bool,
}

impl Default for ApplicationState {
    fn default() -> Self {
        ApplicationState {
            device: None,
            vault: None,
            join_component: false,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn meta_password_id() {
        let pass_id = MetaPasswordId::build("test".to_string(), "salt".to_string());
        assert_eq!(pass_id.id, "CHKANX39xaMXfhe3Qkx9-w".to_string())
    }
}
