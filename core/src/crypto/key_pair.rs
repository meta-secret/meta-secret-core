use crypto_box::aead::OsRng as CryptoBoxOsRng;
use ed25519_dalek::{SecretKey, Signer, SigningKey};
use rand::rngs::OsRng as RandOsRng;
use rand::RngCore;

use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::keys::{DsaPk, DsaSk, TransportPk, TransportSk};
use crate::node::common::model::crypto::aead::{AeadAuthData, AeadCipherText, AeadPlainText};
use crate::node::common::model::crypto::channel::CommunicationChannel;
use crate::secret::shared_secret::PlainText;
use crate::CoreResult;

pub type CryptoBoxPublicKey = crypto_box::PublicKey;
pub type CryptoBoxSecretKey = crypto_box::SecretKey;

pub type DalekKeyPair = SigningKey;
pub type DalekPublicKey = ed25519_dalek::VerifyingKey;
pub type DalekSecretKey = SecretKey;
pub type DalekSignature = ed25519_dalek::Signature;

pub trait KeyPair<Pk, Sk> {
    fn generate() -> Self;
    fn pk(&self) -> Pk;
    fn sk(&self) -> Sk;
}

pub struct DsaKeyPair {
    pub key_pair: DalekKeyPair,
}

impl DsaKeyPair {
    pub fn sign(&self, text: String) -> Base64Text {
        let signature: DalekSignature = self.key_pair.sign(text.as_bytes());
        Base64Text::from(&signature)
    }

    pub fn encode_key_pair(&self) -> Base64Text {
        Base64Text::from(self.key_pair.to_bytes().as_slice())
    }
}

impl KeyPair<DsaPk, DsaSk> for DsaKeyPair {
    fn generate() -> Self {
        let sk_arr = {
            let mut sk_bytes: [u8; 32] = [0; 32];

            let mut cs_prng = RandOsRng {};
            cs_prng.fill_bytes(&mut sk_bytes);
            sk_bytes
        };

        let signing_key = {
            let sk = SecretKey::from(sk_arr);
            SigningKey::from_bytes(&sk)
        };

        DsaKeyPair {
            key_pair: signing_key,
        }
    }

    fn pk(&self) -> DsaPk {
        let pk = Base64Text::from(&self.key_pair.verifying_key());
        DsaPk(pk)
    }

    fn sk(&self) -> DsaSk {
        let raw_sk = Base64Text::from(&self.key_pair.to_bytes());
        DsaSk(raw_sk)
    }
}

pub struct TransportDsaKeyPair {
    pub secret_key: CryptoBoxSecretKey,
}

impl TransportDsaKeyPair {
    pub fn sk(&self) -> TransportSk {
        let raw_sk = Base64Text::from(self.secret_key.as_bytes());
        TransportSk(raw_sk)
    }
}

impl KeyPair<TransportPk, TransportSk> for TransportDsaKeyPair {
    fn generate() -> Self {
        let secret_key = CryptoBoxSecretKey::generate(&mut CryptoBoxOsRng);
        Self { secret_key }
    }

    fn pk(&self) -> TransportPk {
        TransportPk::from(&self.secret_key.public_key())
    }

    fn sk(&self) -> TransportSk {
        let raw_sk = Base64Text::from(self.secret_key.as_bytes());
        TransportSk(raw_sk)
    }
}

pub trait Cipher {
    fn encrypt(&self, plain_text: &AeadPlainText) -> CoreResult<AeadCipherText>;
    fn decrypt(&self, cipher_text: &AeadCipherText) -> CoreResult<AeadPlainText>;
}

impl TransportDsaKeyPair {
    pub fn encrypt_string(
        &self,
        plain_text: PlainText,
        receiver_pk: &TransportPk,
    ) -> anyhow::Result<AeadCipherText> {
        let channel = CommunicationChannel::build(self.pk(), receiver_pk.clone());

        let plain_text = AeadPlainText {
            msg: Base64Text::from(plain_text),
            auth_data: AeadAuthData::from(channel),
        };

        plain_text.encrypt(&self.secret_key)
    }
}

#[cfg(test)]
pub mod test {
    use crate::crypto::encoding::base64::Base64Text;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::node::common::model::crypto::aead::{AeadAuthData, AeadCipherText, AeadPlainText};
    use crate::node::common::model::crypto::channel::CommunicationChannel;
    use crate::secret::shared_secret::PlainText;

    #[test]
    fn single_person_encryption() -> anyhow::Result<()> {
        let password = PlainText::from("topSecret");

        let alice_km = KeyManager::generate();
        let cipher_text: AeadCipherText = alice_km
            .transport
            .encrypt_string(password.clone(), &alice_km.transport.pk())?;

        let plain_text = cipher_text.decrypt(&alice_km.transport.sk())?;
        assert_eq!(Base64Text::from(password), plain_text.msg);

        Ok(())
    }

    #[test]
    fn straight_and_backward_decryption() -> anyhow::Result<()> {
        let alice_km = KeyManager::generate();
        let bob_km = KeyManager::generate();

        let channel = CommunicationChannel::build(alice_km.transport.pk(), bob_km.transport.pk());

        let plain_text = {
            let auth_data = AeadAuthData::from(channel);
            AeadPlainText {
                msg: Base64Text::from("t0p$3cr3t"),
                auth_data,
            }
        };

        let cipher_text = plain_text.encrypt(&alice_km.transport.secret_key)?;

        let decrypted_text = cipher_text.decrypt(&bob_km.transport.sk())?;
        assert_eq!(plain_text, decrypted_text);

        let decrypted_text = cipher_text.decrypt(&alice_km.transport.sk())?;
        assert_eq!(plain_text, decrypted_text);

        let cipher_text = AeadCipherText {
            msg: cipher_text.msg,
            auth_data: cipher_text.auth_data.with_inverse_channel(),
        };

        let decrypted_text = cipher_text.decrypt(&bob_km.transport.sk())?;

        assert_eq!(plain_text.msg, decrypted_text.msg);

        Ok(())
    }

    #[test]
    fn third_party_decryption() -> anyhow::Result<()> {
        let alice_km = KeyManager::generate();
        let bob_km = KeyManager::generate();

        let cipher_text: AeadCipherText = alice_km
            .transport
            .encrypt_string(PlainText::from("secret"), &alice_km.transport.pk())?;

        let error_result = cipher_text.decrypt(&bob_km.transport.sk());
        let error = error_result.is_err();

        assert!(error);

        Ok(())
    }
}
