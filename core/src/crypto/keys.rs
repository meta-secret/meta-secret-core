use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::{DsaKeyPair, KeyPair, MetaPublicKey, TransportDsaKeyPair};
use crate::node::common::model::crypto::{AeadPlainText, CipherEndpoint, CipherLink, EncryptedMessage};
use anyhow::Result;
use wasm_bindgen::prelude::wasm_bindgen;

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
pub struct DsaPk(Base64Text);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct TransportPk(Base64Text);

/// Serializable version of KeyManager
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretBox {
    pub dsa: SerializedDsaKeyPair,
    pub transport: SerializedTransportKeyPair,
}

impl OpenBox {
    pub fn transport_pk(&self) -> MetaPublicKey {
        MetaPublicKey(self.transport_pk.clone())
    }
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

impl SecretBox {
    pub fn re_encrypt(&self, message: EncryptedMessage, receiver: CipherEndpoint) -> Result<EncryptedMessage> {
        let key_manager = KeyManager::try_from(self)?;

        let AeadPlainText { msg: Base64Text(plain_msg), .. } = {
            let sk = &key_manager.transport.secret_key;
            message.cipher_text().decrypt(sk)?
        };

        let new_cypher_text = key_manager.transport.encrypt_string(plain_msg, receiver.pk.clone())?;

        let cipher_link = CipherLink {
            sender: message.cipher_link().find_endpoint(key_manager.transport.public_key())?,
            receiver,
        };

        Ok(EncryptedMessage::CipherShare { cipher_link, share: new_cypher_text })
    }
}