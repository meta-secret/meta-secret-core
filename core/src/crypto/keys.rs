use wasm_bindgen::prelude::wasm_bindgen;
use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::{DsaKeyPair, KeyPair, TransportDsaKeyPair};

pub struct KeyManager {
    pub dsa: DsaKeyPair,
    pub transport: TransportDsaKeyPair,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenBox {
    pub dsa_pk: Base64Text,
    pub transport_pk: Base64Text,
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
    pub key_pair: Base64Text,
    pub public_key: Base64Text,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedTransportKeyPair {
    pub secret_key: Base64Text,
    pub public_key: Base64Text,
}

/// Key manager can be used only with a single vault name (in the future they will be independent entities)
impl KeyManager {
    pub fn generate() -> KeyManager {
        KeyManager {
            dsa: DsaKeyPair::generate(),
            transport: TransportDsaKeyPair::generate(),
        }
    }

    pub fn generate_secret_box() -> SecretBox {
        let key_manager = KeyManager::generate();
        SecretBox::from(key_manager)
    }
}
