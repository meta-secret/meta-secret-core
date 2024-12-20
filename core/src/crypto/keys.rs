use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::{CryptoBoxPublicKey, CryptoBoxSecretKey, DsaKeyPair, KeyPair, TransportDsaKeyPair};
use crate::node::common::model::crypto::aead::{AeadPlainText, EncryptedMessage};
use anyhow::Result;
use wasm_bindgen::prelude::wasm_bindgen;
use crate::crypto::encoding::Array256Bit;
use crate::crypto::utils::U64IdUrlEnc;
use crate::node::common::model::crypto::channel::{CommunicationChannel, LoopbackChannel};
use crate::node::common::model::device::common::DeviceId;
use crate::secret::shared_secret::PlainText;

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
        let pk_id = U64IdUrlEnc::from(self.0.base64_str());
        DeviceId(pk_id)
    }

    pub fn to_loopback_channel(self) -> LoopbackChannel {
        CommunicationChannel::single_device(self)
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

impl TransportSk {
    pub fn pk(&self) -> Result<TransportPk> {
        let sk = self.as_crypto_box_sk()?;
        Ok(TransportPk::from(&sk.public_key()))
    }

    pub fn as_crypto_box_sk(&self) -> Result<CryptoBoxSecretKey> {
        let sk = CryptoBoxSecretKey::try_from(&self.0)?;
        Ok(sk)
    }
}

/// Serializable version of KeyManager
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretBox {
    pub dsa: SerializedDsaKeyPair,
    pub transport: SerializedTransportKeyPair,
}

impl OpenBox {
    pub fn transport_pk(&self) -> &TransportPk {
        &self.transport_pk
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
        SecretBox::from(&key_manager)
    }
}

impl SecretBox {
    pub fn re_encrypt(&self, message: EncryptedMessage, receiver: &TransportPk) -> Result<EncryptedMessage> {
        let key_manager = KeyManager::try_from(self)?;

        let AeadPlainText { msg, .. } = {
            let sk = &key_manager.transport.sk();
            message.cipher_text().decrypt(sk)?
        };

        let new_cypher_text = key_manager.transport
            .encrypt_string(PlainText::from(&msg), receiver)?;

        Ok(EncryptedMessage::CipherShare { share: new_cypher_text })
    }
}


#[cfg(test)]
mod test {
    use anyhow::Result;
    use crate::crypto::encoding::base64::Base64Text;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::{KeyManager, SecretBox};
    use crate::node::common::model::crypto::aead::EncryptedMessage;
    use crate::secret::shared_secret::PlainText;

    #[test]
    fn re_encrypt_test() -> Result<()> {
        let plain_text = PlainText::from("2bee~");
        let base64_plaint_text = &Base64Text::from(plain_text.clone());
        
        let alice_km = KeyManager::generate();
        let alice_sk = &alice_km.transport.sk();
        let alice_secret_box = SecretBox::from(&alice_km);
        
        let bob_km = KeyManager::generate();
        let bob_sk = &bob_km.transport.sk();
        
        let msg_1 = {
            let cypher_text = alice_km.transport
                .encrypt_string(plain_text, &alice_km.transport.pk())?;
            EncryptedMessage::CipherShare {share: cypher_text}
        };

        let msg_1_alice_aead_plain_text = msg_1.cipher_text().decrypt(alice_sk)?;
        assert_eq!(base64_plaint_text, &msg_1_alice_aead_plain_text.msg);
        
        let msg_2 = alice_secret_box.re_encrypt(msg_1, &bob_km.transport.pk())?;
        let msg_2_alice_aead_plain_text = msg_2.cipher_text().decrypt(alice_sk)?;
        let msg_2_bob_aead_plain_text = msg_2.cipher_text().decrypt(bob_sk)?;
        
        assert_eq!(base64_plaint_text, &msg_2_alice_aead_plain_text.msg);
        assert_eq!(base64_plaint_text, &msg_2_bob_aead_plain_text.msg);
        
        Ok(())
    }
}