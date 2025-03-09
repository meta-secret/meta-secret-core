use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::{DsaKeyPair, KeyPair, TransportDsaKeyPair};
use crate::crypto::utils::U64IdUrlEnc;
use crate::node::common::model::crypto::aead::{AeadPlainText, EncryptedMessage};
use crate::node::common::model::crypto::channel::{CommunicationChannel, LoopbackChannel};
use crate::node::common::model::device::common::DeviceId;
use crate::secret::shared_secret::PlainText;
use age::x25519::{Identity, Recipient};
use anyhow::{anyhow, bail, Result};
use std::str::FromStr;
use wasm_bindgen::prelude::wasm_bindgen;

pub struct KeyManager {
    pub dsa: DsaKeyPair,
    pub transport: TransportDsaKeyPair,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct OpenBox {
    pub dsa_pk: DsaPk,
    pub transport_pk: TransportPk,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct DsaPk(pub Base64Text);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct DsaSk(pub Base64Text);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct TransportPk(Base64Text);

impl TransportPk {
    pub fn to_device_id(&self) -> DeviceId {
        let pk_id = U64IdUrlEnc::from(self.0.base64_str());
        DeviceId(pk_id)
    }

    pub fn to_loopback_channel(self) -> LoopbackChannel {
        CommunicationChannel::single_device(self)
    }

    pub fn as_recipient(&self) -> Result<Recipient> {
        let pk_base64 = &self.0;
        let pk_str = String::try_from(pk_base64)?;
        Recipient::from_str(pk_str.as_str()).map_err(|err_str| anyhow!(err_str))
    }
}

impl From<Base64Text> for TransportPk {
    fn from(pk_b64: Base64Text) -> Self {
        Self(pk_b64)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct TransportSk(pub Base64Text);

impl TransportSk {
    pub fn pk(&self) -> Result<TransportPk> {
        let sk = self.as_age()?;
        let pk = Base64Text::from(sk.to_public().to_string());
        Ok(TransportPk::from(pk))
    }

    pub fn as_age(&self) -> Result<Identity> {
        let decoded_sk = String::try_from(&self.0)?;
        let sk_result = Identity::from_str(decoded_sk.as_str());
        match sk_result {
            Ok(sk) => Ok(sk),
            Err(err_str) => {
                bail!(err_str)
            }
        }
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
    pub fn re_encrypt(
        &self,
        message: EncryptedMessage,
        receiver: &TransportPk,
    ) -> Result<EncryptedMessage> {
        let key_manager = KeyManager::try_from(self)?;

        let AeadPlainText { msg, .. } = {
            let sk = &key_manager.transport.sk();
            message.cipher_text().decrypt(sk)?
        };

        let new_cypher_text = key_manager
            .transport
            .encrypt_string(PlainText::from(&msg), receiver)?;

        Ok(EncryptedMessage::CipherShare {
            share: new_cypher_text,
        })
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::encoding::base64::Base64Text;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::{KeyManager, SecretBox};
    use crate::node::common::model::crypto::aead::EncryptedMessage;
    use crate::secret::shared_secret::PlainText;
    use anyhow::Result;

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
            let cypher_text = alice_km
                .transport
                .encrypt_string(plain_text, &alice_km.transport.pk())?;
            EncryptedMessage::CipherShare { share: cypher_text }
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

#[cfg(test)]
pub mod fixture {
    use crate::crypto::keys::KeyManager;

    pub struct KeyManagerFixture {
        pub client: KeyManager,
        pub client_b: KeyManager,
        pub vd: KeyManager,
        pub server: KeyManager,
    }

    impl KeyManagerFixture {
        pub fn generate() -> Self {
            Self {
                client: KeyManager::generate(),
                client_b: KeyManager::generate(),
                vd: KeyManager::generate(),
                server: KeyManager::generate(),
            }
        }
    }
}
