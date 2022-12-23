use crate::crypto::{
    encoding::{base64::Base64EncodedText, serialized_key_manager::SerializedKeyManager},
    key_pair::{DsaKeyPair, KeyPair, TransportDsaKeyPair},
};
use crate::sdk::vault::UserSignature;
use serde::{Deserialize, Serialize};
use meta_secret_core_models::models::DeviceInfo;

pub struct KeyManager {
    pub dsa: DsaKeyPair,
    pub transport_key_pair: TransportDsaKeyPair,
}

/// Key manager can be used only with a single vault name (in the future they will be independent entities)
impl KeyManager {
    pub fn generate() -> KeyManager {
        KeyManager {
            dsa: DsaKeyPair::generate(),
            transport_key_pair: TransportDsaKeyPair::generate(),
        }
    }

    pub fn generate_security_box(vault_name: String) -> UserSecurityBox {
        let key_manager = KeyManager::generate();

        let signature = key_manager.dsa.sign(vault_name.clone());
        UserSecurityBox {
            vault_name,
            signature,
            key_manager: SerializedKeyManager::from(&key_manager),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSecurityBox {
    pub vault_name: String,
    pub signature: Base64EncodedText,
    pub key_manager: SerializedKeyManager,
}

impl UserSecurityBox {
    pub fn get_user_sig(&self, device: &DeviceInfo) -> UserSignature {
        let key_manager: KeyManager = KeyManager::try_from(&self.key_manager).unwrap();

        UserSignature {
            vault_name: self.vault_name.clone(),
            device: device.clone(),
            public_key: key_manager.dsa.public_key(),
            transport_public_key: key_manager.transport_key_pair.public_key(),
            signature: key_manager.dsa.sign(self.vault_name.clone()),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AeadPlainText {
    pub msg: Base64EncodedText,
    pub auth_data: AeadAuthData,
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AeadCipherText {
    pub msg: Base64EncodedText,
    pub auth_data: AeadAuthData,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AeadAuthData {
    pub associated_data: String,
    pub channel: CommunicationChannel,
    pub nonce: Base64EncodedText,
}

/// Represents virtual ncrypted communication channel between two points.
/// Contains public keys of a sender and a receiver
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunicationChannel {
    pub sender: Base64EncodedText,
    pub receiver: Base64EncodedText,
}

impl CommunicationChannel {
    pub fn inverse(self) -> Self {
        Self {
            sender: self.receiver,
            receiver: self.sender,
        }
    }
}
