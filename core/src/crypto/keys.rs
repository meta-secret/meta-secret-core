use crate::crypto::key_pair::{DsaKeyPair, KeyPair, TransportDsaKeyPair};
use crate::models::{Base64EncodedText, CommunicationChannel};

pub struct KeyManager {
    pub dsa: DsaKeyPair,
    pub transport_key_pair: TransportDsaKeyPair,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenBox {
    pub dsa_pk: Base64EncodedText,
    pub transport_pk: Base64EncodedText,
}

/// Serializable version of KeyManager
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretBox {
    pub dsa: SerializedDsaKeyPair,
    pub transport: SerializedTransportKeyPair,
}

impl From<&SecretBox> for OpenBox {
    fn from(secret_box: &SecretBox) -> Self {
        Self {
            dsa_pk: secret_box.dsa.public_key.clone(),
            transport_pk: secret_box.transport.public_key.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedDsaKeyPair {
    pub key_pair: Base64EncodedText,
    pub public_key: Base64EncodedText,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedTransportKeyPair {
    pub secret_key: Base64EncodedText,
    pub public_key: Base64EncodedText,
}


/// Key manager can be used only with a single vault name (in the future they will be independent entities)
impl KeyManager {
    pub fn generate() -> KeyManager {
        KeyManager {
            dsa: DsaKeyPair::generate(),
            transport_key_pair: TransportDsaKeyPair::generate(),
        }
    }

    pub fn generate_secret_box() -> SecretBox {
        let key_manager = KeyManager::generate();
        SecretBox::from(key_manager)
    }
}

impl CommunicationChannel {
    pub fn inverse(self) -> Self {
        Self {
            sender: self.receiver,
            receiver: self.sender,
        }
    }
}
