
use crypto_box::{
    aead::{Aead, OsRng as CryptoBoxOsRng, Payload},
    ChaChaBox, Nonce,
};
use crypto_box::aead::AeadCore;
use ed25519_dalek::{Keypair, Signer};
use image::EncodableLayout;
use rand::RngCore;
use rand::rngs::OsRng as RandOsRng;

use crate::CoreResult;
use crate::crypto::encoding::base64::Base64Text;
use crate::errors::CoreError;
use crate::node::common::model::crypto::{AeadAuthData, AeadCipherText, AeadPlainText, CommunicationChannel};

pub type CryptoBoxPublicKey = crypto_box::PublicKey;
pub type CryptoBoxSecretKey = crypto_box::SecretKey;

pub type DalekKeyPair = ed25519_dalek::Keypair;
pub type DalekPublicKey = ed25519_dalek::PublicKey;
pub type DalekSecretKey = ed25519_dalek::SecretKey;
pub type DalekSignature = ed25519_dalek::Signature;

pub trait KeyPair {
    fn generate() -> Self;
    fn public_key(&self) -> Base64Text;
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
            key_pair: Keypair { public: pk, secret: sk },
        }
    }

    fn public_key(&self) -> Base64Text {
        Base64Text::from(&self.key_pair.public)
    }

    fn secret_key(&self) -> Base64Text {
        Base64Text::from(&self.key_pair.secret.to_bytes())
    }
}

pub struct TransportDsaKeyPair {
    pub secret_key: CryptoBoxSecretKey,
    pub public_key: CryptoBoxPublicKey,
}

impl KeyPair for TransportDsaKeyPair {
    fn generate() -> Self {
        let secret_key = CryptoBoxSecretKey::generate(&mut CryptoBoxOsRng);
        let public_key = secret_key.public_key();

        Self { secret_key, public_key }
    }

    fn public_key(&self) -> Base64Text {
        Base64Text::from(self.public_key.as_bytes())
    }

    fn secret_key(&self) -> Base64Text {
        Base64Text::from(self.secret_key.as_bytes())
    }
}

impl TransportDsaKeyPair {
    pub fn build_cha_cha_box(&self, their_pk: &CryptoBoxPublicKey) -> ChaChaBox {
        ChaChaBox::new(their_pk, &self.secret_key)
    }

    pub fn encrypt_string(&self, plain_text: String, receiver_pk: Base64Text) -> CoreResult<AeadCipherText> {
        let channel = CommunicationChannel {
            sender: self.public_key(),
            receiver: receiver_pk,
        };
        let auth_data = AeadAuthData {
            associated_data: "checksum".to_string(),
            channel,
            nonce: self.generate_nonce(),
        };
        let aead_text = AeadPlainText {
            msg: Base64Text::from(plain_text.as_bytes()),
            auth_data,
        };

        self.encrypt(&aead_text)
    }

    pub fn encrypt(&self, plain_text: &AeadPlainText) -> CoreResult<AeadCipherText> {
        let auth_data = &plain_text.auth_data;

        let their_pk = CryptoBoxPublicKey::try_from(&auth_data.channel.receiver)?;

        let crypto_box = self.build_cha_cha_box(&their_pk);

        let nonce = Nonce::try_from(&auth_data.nonce)?;
        let msg = Vec::try_from(&plain_text.msg)?;
        let payload = Payload {
            msg: msg.as_bytes(),                       // your message to encrypt
            aad: auth_data.associated_data.as_bytes(), // not encrypted, but authenticated in tag
        };
        let cipher_text = crypto_box.encrypt(&nonce, payload)?;

        let cipher_text = AeadCipherText {
            msg: Base64Text::from(cipher_text),
            auth_data: plain_text.auth_data.clone(),
        };

        Ok(cipher_text)
    }

    pub fn decrypt(&self, cipher_text: &AeadCipherText) -> CoreResult<AeadPlainText> {
        let auth_data = &cipher_text.auth_data;
        let channel = &auth_data.channel;

        let owner_pk = self.public_key();

        let their_pk = match owner_pk {
            pk if pk == channel.sender => {
                CryptoBoxPublicKey::try_from(&channel.receiver)
            }
            pk if pk == channel.receiver => {
                CryptoBoxPublicKey::try_from(&channel.sender)
            }
            _ => Err(CoreError::ThirdPartyEncryptionError {
                key_manager_pk: owner_pk,
                channel: channel.clone(),
            }),
        }?;

        let crypto_box = self.build_cha_cha_box(&their_pk);

        let msg_vec: Vec<u8> = Vec::try_from(&cipher_text.msg)?;
        let nonce = Nonce::try_from(&auth_data.nonce)?;
        let payload = Payload {
            msg: msg_vec.as_bytes(),
            aad: auth_data.associated_data.as_bytes(),
        };
        let decrypted_plaintext = crypto_box.decrypt(&nonce, payload)?;

        let plain_text = AeadPlainText {
            msg: Base64Text::from(decrypted_plaintext),
            auth_data: cipher_text.auth_data.clone(),
        };

        Ok(plain_text)
    }

    pub fn generate_nonce(&self) -> Base64Text {
        let nonce: Nonce = ChaChaBox::generate_nonce(&mut CryptoBoxOsRng);
        Base64Text::from(nonce.as_slice())
    }
}

#[cfg(test)]
pub mod test {
    use crate::CoreResult;
    use crate::crypto::encoding::base64::Base64Text;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::errors::CoreError;
    use crate::node::common::model::crypto::{AeadAuthData, AeadCipherText, AeadPlainText, CommunicationChannel};

    #[test]
    fn single_person_encryption() -> CoreResult<()> {
        let password = "topSecret".to_string();

        let alice_km = KeyManager::generate();
        let cipher_text: AeadCipherText = alice_km
            .transport_key_pair
            .encrypt_string(password.clone(), alice_km.transport_key_pair.public_key())?;

        let plain_text = alice_km.transport_key_pair.decrypt(&cipher_text)?;
        assert_eq!(
            Base64Text::from(password.as_bytes()),
            plain_text.msg
        );

        Ok(())
    }

    #[test]
    fn straight_and_backward_decryption() -> CoreResult<()> {
        let alice_km = KeyManager::generate();
        let bob_km = KeyManager::generate();

        let channel = CommunicationChannel {
            sender: alice_km.transport_key_pair.public_key(),
            receiver: bob_km.transport_key_pair.public_key(),
        };

        let plain_text = {
            let nonce = alice_km.transport_key_pair.generate_nonce();

            let auth_data = AeadAuthData {
                associated_data: "checksum".to_string(),
                channel,
                nonce,
            };

            AeadPlainText {
                msg: Base64Text::from("t0p$3cr3t"),
                auth_data,
            }
        };

        let cipher_text = alice_km.transport_key_pair.encrypt(&plain_text)?;

        let decrypted_text = bob_km.transport_key_pair.decrypt(&cipher_text)?;
        assert_eq!(plain_text, decrypted_text);

        let decrypted_text = alice_km.transport_key_pair.decrypt(&cipher_text)?;
        assert_eq!(plain_text, decrypted_text);

        let cipher_text = AeadCipherText {
            msg: cipher_text.msg,
            auth_data: AeadAuthData {
                associated_data: cipher_text.auth_data.associated_data,
                channel: cipher_text.auth_data.channel.inverse(),
                nonce: cipher_text.auth_data.nonce,
            }
        };

        let decrypted_text = bob_km.transport_key_pair.decrypt(&cipher_text)?;

        assert_eq!(plain_text.msg, decrypted_text.msg);

        Ok(())
    }

    #[test]
    fn third_party_decryption() -> CoreResult<()> {
        let alice_km = KeyManager::generate();
        let bob_km = KeyManager::generate();

        let cipher_text: AeadCipherText = alice_km
            .transport_key_pair
            .encrypt_string("secret".to_string(), alice_km.transport_key_pair.public_key())?;

        let error_result = bob_km.transport_key_pair.decrypt(&cipher_text);
        let error = error_result.unwrap_err();

        match error {
            CoreError::ThirdPartyEncryptionError {
                key_manager_pk,
                channel,
            } => {
                assert_eq!(key_manager_pk, bob_km.transport_key_pair.public_key());
                assert_eq!(channel, cipher_text.auth_data.channel)
            }
            _ => panic!("Critical error"),
        }

        Ok(())
    }
}
