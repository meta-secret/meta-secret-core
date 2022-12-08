use crate::crypto::encoding::base64::Base64EncodedText;
use crate::crypto::key_pair::{DsaKeyPair, KeyPair, TransportDsaKeyPair};
use serde::{Deserialize, Serialize};

pub struct KeyManager {
    pub dsa: DsaKeyPair,
    pub transport_key_pair: TransportDsaKeyPair,
}

impl KeyManager {
    pub fn generate() -> KeyManager {
        KeyManager {
            dsa: DsaKeyPair::generate(),
            transport_key_pair: TransportDsaKeyPair::generate(),
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
