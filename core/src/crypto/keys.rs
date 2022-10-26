use crate::crypto::encoding::Base64EncodedText;
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
    /// can be plain text or cypher text
    pub msg: String,
    pub auth_data: AeadAuthData,
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AeadCipherText {
    /// can be plain text or cypher text
    pub msg: Base64EncodedText,
    pub auth_data: AeadAuthData,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AeadAuthData {
    pub associated_data: String,
    pub sender_public_key: Base64EncodedText,
    pub receiver_public_key: Base64EncodedText,
    pub nonce: Base64EncodedText,
}
