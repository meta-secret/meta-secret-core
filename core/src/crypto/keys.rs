use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::{CryptoBoxPublicKey, DsaKeyPair, KeyPair, TransportDsaKeyPair};
use crate::node::common::model::crypto::{AeadPlainText, EncryptedMessage};
use anyhow::Result;
use wasm_bindgen::prelude::wasm_bindgen;
use crate::crypto::encoding::Array256Bit;
use crate::crypto::utils::U64IdUrlEnc;
use crate::node::common::model::device::common::DeviceId;

pub struct KeyManager {
    pub dsa: DsaKeyPair,
    pub transport: TransportDsaKeyPair,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct OpenBox {
    pub dsa_pk: DsaPk,
    pub transport_pk: TransportPk,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct DsaPk(pub Base64Text);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct DsaSk(pub Base64Text);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct TransportPk(Base64Text);

impl TransportPk {
    pub fn as_crypto_box_pk(&self) -> Result<CryptoBoxPublicKey> {
        let byte_array = Array256Bit::try_from(&self.0)?;
        Ok(CryptoBoxPublicKey::from(byte_array))
    }
    
    pub fn to_device_id(&self) -> DeviceId {
        let pk_id = U64IdUrlEnc::from(self.0.0.clone());
        DeviceId(pk_id)
    }
}

impl From<&CryptoBoxPublicKey> for TransportPk {
    fn from(pk: &CryptoBoxPublicKey) -> Self {
        let pk_url_enc = Base64Text::from(pk.as_bytes());
        Self(pk_url_enc)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct TransportSk(pub Base64Text);

/// Serializable version of KeyManager
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretBox {
    pub dsa: SerializedDsaKeyPair,
    pub transport: SerializedTransportKeyPair,
}

impl OpenBox {
    pub fn transport_pk(&self) -> TransportPk {
        self.transport_pk.clone()
    }
}

impl From<&SecretBox> for OpenBox {
    fn from(secret_box: &SecretBox) -> Self {
        Self {
            dsa_pk: secret_box.dsa.public_key.clone(),
            transport_pk: secret_box.transport.pk.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedDsaKeyPair {
    pub key_pair: Base64Text,
    pub public_key: DsaPk,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedTransportKeyPair {
    pub sk: TransportSk,
    pub pk: TransportPk,
}

/// Key manager can be used only with a single vault name
/// (in the future they will be independent entities)
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

impl SecretBox {
    pub fn re_encrypt(&self, message: EncryptedMessage, receiver: TransportPk) -> Result<EncryptedMessage> {
        let key_manager = KeyManager::try_from(self)?;

        let AeadPlainText { msg: Base64Text(plain_msg), .. } = {
            let sk = &key_manager.transport.secret_key;
            message.cipher_text().decrypt(sk)?
        };

        let new_cypher_text = key_manager.transport.encrypt_string(plain_msg, receiver)?;

        Ok(EncryptedMessage::CipherShare { share: new_cypher_text })
    }
}
