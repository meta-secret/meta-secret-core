use crypto_box::aead::OsRng as CryptoBoxOsRng;
use ed25519_dalek::{Keypair, Signer};
use rand::rngs::OsRng as RandOsRng;
use rand::RngCore;

use crate::crypto::encoding::base64::Base64Text;
use crate::node::common::model::crypto::{
    AeadAuthData, AeadCipherText, AeadPlainText, CommunicationChannel,
};
use crate::CoreResult;

pub type CryptoBoxPublicKey = crypto_box::PublicKey;
pub type CryptoBoxSecretKey = crypto_box::SecretKey;

pub type DalekKeyPair = Keypair;
pub type DalekPublicKey = ed25519_dalek::PublicKey;
pub type DalekSecretKey = ed25519_dalek::SecretKey;
pub type DalekSignature = ed25519_dalek::Signature;

pub trait KeyPair {
    fn generate() -> Self;
    fn public_key(&self) -> MetaPublicKey;
    fn secret_key(&self) -> Base64Text;
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

impl KeyPair for DsaKeyPair {
    fn generate() -> Self {
        let mut sk_arr: [u8; 32] = [0; 32];

        let mut cs_prng = RandOsRng {};
        cs_prng.fill_bytes(&mut sk_arr);

        let sk = DalekSecretKey::from_bytes(&sk_arr).unwrap();
        let pk = DalekPublicKey::from(&sk);

        DsaKeyPair {
            key_pair: Keypair {
                public: pk,
                secret: sk,
            },
        }
    }

    fn public_key(&self) -> MetaPublicKey {
        let pk = Base64Text::from(&self.key_pair.public);
        MetaPublicKey(pk)
    }

    fn secret_key(&self) -> Base64Text {
        Base64Text::from(&self.key_pair.secret.to_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaPublicKey(pub Base64Text);

pub struct TransportDsaKeyPair {
    pub secret_key: CryptoBoxSecretKey,
}

impl KeyPair for TransportDsaKeyPair {
    fn generate() -> Self {
        let secret_key = CryptoBoxSecretKey::generate(&mut CryptoBoxOsRng);
        Self { secret_key }
    }

    fn public_key(&self) -> MetaPublicKey {
        let pk = Base64Text::from(self.secret_key.public_key().as_bytes());
        MetaPublicKey(pk)
    }

    fn secret_key(&self) -> Base64Text {
        Base64Text::from(self.secret_key.as_bytes())
    }
}

pub trait Cipher {
    fn encrypt(&self, plain_text: &AeadPlainText) -> CoreResult<AeadCipherText>;
    fn decrypt(&self, cipher_text: &AeadCipherText) -> CoreResult<AeadPlainText>;
}

impl TransportDsaKeyPair {
    pub fn encrypt_string(
        &self,
        plain_text: String,
        receiver_pk: MetaPublicKey,
    ) -> CoreResult<AeadCipherText> {
        let channel = CommunicationChannel {
            sender: self.public_key(),
            receiver: receiver_pk,
        };
        let auth_data = AeadAuthData::from(channel);
        let plain_text = AeadPlainText {
            msg: Base64Text::from(plain_text.as_bytes()),
            auth_data,
        };

        plain_text.encrypt(&self.secret_key)
    }
}

#[cfg(test)]
pub mod test {
    use crate::crypto::encoding::base64::Base64Text;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::errors::CoreError;
    use crate::node::common::model::crypto::{
        AeadAuthData, AeadCipherText, AeadPlainText, CommunicationChannel,
    };
    use crate::CoreResult;

    #[test]
    fn single_person_encryption() -> CoreResult<()> {
        let password = "topSecret".to_string();

        let alice_km = KeyManager::generate();
        let cipher_text: AeadCipherText = alice_km
            .transport
            .encrypt_string(password.clone(), alice_km.transport.public_key())?;

        let plain_text = cipher_text.decrypt(&alice_km.transport.secret_key)?;
        assert_eq!(Base64Text::from(password.as_bytes()), plain_text.msg);

        Ok(())
    }

    #[test]
    fn straight_and_backward_decryption() -> CoreResult<()> {
        let alice_km = KeyManager::generate();
        let bob_km = KeyManager::generate();

        let channel = CommunicationChannel {
            sender: alice_km.transport.public_key(),
            receiver: bob_km.transport.public_key(),
        };

        let plain_text = {
            let auth_data = AeadAuthData::from(channel);
            AeadPlainText {
                msg: Base64Text::from("t0p$3cr3t"),
                auth_data,
            }
        };

        let cipher_text = plain_text.encrypt(&alice_km.transport.secret_key)?;

        let decrypted_text = cipher_text.decrypt(&bob_km.transport.secret_key)?;
        assert_eq!(plain_text, decrypted_text);

        let decrypted_text = cipher_text.decrypt(&alice_km.transport.secret_key)?;
        assert_eq!(plain_text, decrypted_text);

        let cipher_text = AeadCipherText {
            msg: cipher_text.msg,
            auth_data: cipher_text.auth_data.with_inverse_channel(),
        };

        let decrypted_text = cipher_text.decrypt(&bob_km.transport.secret_key)?;

        assert_eq!(plain_text.msg, decrypted_text.msg);

        Ok(())
    }

    #[test]
    fn third_party_decryption() -> CoreResult<()> {
        let alice_km = KeyManager::generate();
        let bob_km = KeyManager::generate();

        let cipher_text: AeadCipherText = alice_km
            .transport
            .encrypt_string("secret".to_string(), alice_km.transport.public_key())?;

        let error_result = cipher_text.decrypt(&bob_km.transport.secret_key);
        let error = error_result.unwrap_err();

        match error {
            CoreError::ThirdPartyEncryptionError {
                key_manager_pk,
                channel,
            } => {
                assert_eq!(key_manager_pk, bob_km.transport.public_key());
                assert_eq!(channel, cipher_text.auth_data.channel().clone())
            }
            _ => panic!("Critical error"),
        }

        Ok(())
    }
}
