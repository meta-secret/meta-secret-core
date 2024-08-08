use std::fmt::Display;

use crate::node::common::model::device::DeviceData;
use crate::node::common::model::secret::MetaPasswordId;
use crate::node::common::model::vault::VaultStatus;

pub mod device;
pub mod user;
pub mod vault;

pub mod crypto {
    use crate::CoreResult;
    use crypto_box::aead::{Aead, AeadCore};
    use crypto_box::{
        aead::{OsRng as CryptoBoxOsRng, Payload},
        ChaChaBox, Nonce,
    };
    use image::EncodableLayout;

    use crate::crypto::encoding::base64::Base64Text;
    use crate::crypto::key_pair::{CryptoBoxPublicKey, CryptoBoxSecretKey};
    use crate::errors::CoreError;
    use crate::node::common::model::device::DeviceLink;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AeadAuthData {
        associated_data: String,
        channel: CommunicationChannel,
        nonce: Base64Text,
    }

    impl AeadAuthData {
        pub fn with_inverse_channel(&self) -> Self {
            Self {
                associated_data: self.associated_data.clone(),
                channel: self.channel.inverse(),
                nonce: self.nonce.clone(),
            }
        }

        pub fn channel(&self) -> &CommunicationChannel {
            &self.channel
        }

        pub fn generate_nonce() -> Base64Text {
            let nonce: Nonce = ChaChaBox::generate_nonce(&mut CryptoBoxOsRng);
            Base64Text::from(nonce.as_slice())
        }

        pub fn receiver(&self) -> Result<CryptoBoxPublicKey, CoreError> {
            CryptoBoxPublicKey::try_from(&self.channel.receiver)
        }

        pub fn nonce(&self) -> Result<Nonce, CoreError> {
            Nonce::try_from(&self.nonce)
        }
    }

    impl From<CommunicationChannel> for AeadAuthData {
        fn from(channel: CommunicationChannel) -> Self {
            Self {
                associated_data: String::from("checksum"),
                channel,
                nonce: AeadAuthData::generate_nonce(),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AeadCipherText {
        pub msg: Base64Text,
        pub auth_data: AeadAuthData,
    }

    impl AeadCipherText {
        /// Decrypt this secret message using the secret key
        pub fn decrypt(&self, secret_key: &CryptoBoxSecretKey) -> CoreResult<AeadPlainText> {
            let auth_data = &self.auth_data;

            let their_pk = &auth_data.channel().peer(&secret_key.public_key())?;

            let plain_bytes = {
                let crypto_box = ChaChaBox::new(their_pk, &secret_key);

                let msg_data = Vec::try_from(&self.msg)?;
                let payload = Payload {
                    msg: msg_data.as_bytes(),
                    aad: self.auth_data.associated_data.as_bytes(),
                };
                let nonce = auth_data.nonce()?;

                crypto_box.decrypt(&nonce, payload)?
            };

            let plain_text = AeadPlainText {
                msg: Base64Text::from(plain_bytes),
                auth_data: self.auth_data.clone(),
            };

            Ok(plain_text)
        }
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AeadPlainText {
        pub msg: Base64Text,
        pub auth_data: AeadAuthData,
    }

    impl AeadPlainText {
        pub fn encrypt(&self, secret_key: &CryptoBoxSecretKey) -> CoreResult<AeadCipherText> {
            let auth_data = &self.auth_data;

            let crypto_box = {
                let their_pk = auth_data.receiver()?;
                ChaChaBox::new(&their_pk, secret_key)
            };

            let cipher_text = {
                let msg_data = Vec::try_from(&self.msg)?;
                let payload = Payload {
                    msg: msg_data.as_bytes(), // your message to encrypt
                    aad: auth_data.associated_data.as_bytes(), // not encrypted, but authenticated in tag
                };
                let nonce = auth_data.nonce()?;
                crypto_box.encrypt(&nonce, payload)?
            };

            let cipher_text = AeadCipherText {
                msg: Base64Text::from(cipher_text),
                auth_data: self.auth_data.clone(),
            };

            Ok(cipher_text)
        }
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CommunicationChannel {
        pub sender: Base64Text,
        pub receiver: Base64Text,
    }

    impl CommunicationChannel {
        pub fn inverse(&self) -> Self {
            Self {
                sender: self.receiver.clone(),
                receiver: self.sender.clone(),
            }
        }

        pub fn sender(&self) -> CoreResult<CryptoBoxPublicKey> {
            CryptoBoxPublicKey::try_from(&self.sender)
        }

        pub fn receiver(&self) -> CoreResult<CryptoBoxPublicKey> {
            CryptoBoxPublicKey::try_from(&self.receiver)
        }

        /// Get a peer/opponent to a given entity
        pub fn peer(&self, initiator_pk: &CryptoBoxPublicKey) -> CoreResult<CryptoBoxPublicKey> {
            let sender = self.sender()?;
            let receiver = self.receiver()?;

            let peer_pk = match initiator_pk {
                pk if pk.eq(&sender) => CryptoBoxPublicKey::try_from(&self.receiver),
                pk if pk.eq(&receiver) => CryptoBoxPublicKey::try_from(&self.sender),
                _ => Err(CoreError::ThirdPartyEncryptionError {
                    key_manager_pk: Base64Text::from(initiator_pk.as_bytes()),
                    channel: self.clone(),
                }),
            }?;

            Ok(peer_pk)
        }
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum EncryptedMessage {
        /// There is only one type of encrypted message for now, which is encrypted share of a secret,
        /// and that particular type of message has a device link,
        /// and it used to figure out which vault the message belongs to
        CipherShare {
            device_link: DeviceLink,
            share: AeadCipherText,
        },
    }

    impl EncryptedMessage {
        pub fn device_link(&self) -> DeviceLink {
            match self {
                EncryptedMessage::CipherShare { device_link, .. } => device_link.clone(),
            }
        }

        pub fn cipher_text(&self) -> &AeadCipherText {
            match self {
                EncryptedMessage::CipherShare { share, .. } => share,
            }
        }
    }
}

pub mod secret {
    use std::collections::HashMap;

    use rand::distributions::Alphanumeric;
    use rand::Rng;

    use crate::crypto::utils;
    use crate::node::common::model::crypto::EncryptedMessage;
    use crate::node::common::model::vault::VaultName;

    use super::device::PeerToPeerDeviceLink;

    #[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MetaPasswordId {
        /// SHA256 hash of a salt
        pub id: String,
        /// Random String up to 30 characters, must be unique
        pub salt: String,
        /// Human-readable name given to the password
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

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SSDistributionId {
        pub claim_id: SSDistributionClaimId,
        pub distribution_type: SecretDistributionType,
        pub device_link: PeerToPeerDeviceLink,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SSLedgerData {
        pub claims: HashMap<SSDistributionClaimId, SSDistributionClaim>,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SSDistributionClaim {
        pub vault_name: VaultName,
        pub id: SSDistributionClaimId,
        pub pass_id: MetaPasswordId,
        pub distribution_type: SecretDistributionType,
        pub distribution: HashMap<PeerToPeerDeviceLink, SSDistributionStatus>,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SSDistributionClaimId(pub String);

    impl SSDistributionClaimId {
        pub fn generate() -> Self {
            let id: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(SALT_LENGTH)
                .map(char::from)
                .collect();
            Self(id)
        }
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum SSDistributionStatus {
        Pending,
        /// The sender device has sent the secret share
        Sent,
        /// The receiver device has received the secret
        Delivered,
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
        pub vault_name: VaultName,
        pub pass_id: MetaPasswordId,
        pub secret_message: EncryptedMessage,
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
