use crypto_box::aead::AeadCore;
use crypto_box::{
    aead::{Aead, OsRng as CryptoBoxOsRng, Payload},
    ChaChaBox, Nonce, PublicKey as CryptoBoxPublicKey, SecretKey as CryptoBoxSecretKey,
};
use ed25519_dalek::ed25519::signature::Signature;
use ed25519_dalek::{Keypair, Signer};
use image::EncodableLayout;
use rand::rngs::OsRng;

use crate::crypto::encoding::Base64EncodedText;
use crate::crypto::keys::{AeadAuthData, AeadCipherText, AeadPlainText};

pub trait KeyPair {
    fn generate() -> Self;
    fn public_key(&self) -> Base64EncodedText;
    fn secret_key(&self) -> Base64EncodedText;
}

pub struct DsaKeyPair {
    pub key_pair: Keypair,
}

impl DsaKeyPair {
    pub fn sign(&self, text: String) -> Base64EncodedText {
        let signature = self.key_pair.sign(text.as_bytes());
        Base64EncodedText::from(signature.as_bytes())
    }
}

impl KeyPair for DsaKeyPair {
    fn generate() -> Self {
        let mut cs_prng = OsRng {};
        let key_pair = Keypair::generate(&mut cs_prng);

        DsaKeyPair { key_pair }
    }

    fn public_key(&self) -> Base64EncodedText {
        Base64EncodedText::from(&self.key_pair.public.to_bytes())
    }

    fn secret_key(&self) -> Base64EncodedText {
        Base64EncodedText::from(&self.key_pair.secret.to_bytes())
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

    fn public_key(&self) -> Base64EncodedText {
        Base64EncodedText::from(self.public_key.as_bytes())
    }

    fn secret_key(&self) -> Base64EncodedText {
        Base64EncodedText::from(self.secret_key.as_bytes())
    }
}

impl TransportDsaKeyPair {
    pub fn build_cha_cha_box(&self, their_pk: &CryptoBoxPublicKey) -> ChaChaBox {
        ChaChaBox::new(their_pk, &self.secret_key)
    }

    pub fn encrypt_string(&self, plain_text: String, receiver_pk: Base64EncodedText) -> AeadCipherText {
        let aead_text = AeadPlainText {
            msg: plain_text,
            auth_data: AeadAuthData {
                associated_data: "checksum".to_string(),
                sender_public_key: self.public_key(),
                receiver_public_key: receiver_pk,
                nonce: self.generate_nonce(),
            },
        };

        self.encrypt(&aead_text)
    }

    pub fn encrypt(&self, plain_text: &AeadPlainText) -> AeadCipherText {
        let auth_data = &plain_text.auth_data;
        let receiver_pk = CryptoBoxPublicKey::from(&auth_data.receiver_public_key);
        let crypto_box = self.build_cha_cha_box(&receiver_pk);
        let cipher_text = crypto_box
            .encrypt(
                &Nonce::from(&auth_data.nonce),
                Payload {
                    msg: plain_text.msg.clone().as_bytes(),    // your message to encrypt
                    aad: auth_data.associated_data.as_bytes(), // not encrypted, but authenticated in tag
                },
            )
            .unwrap();

        AeadCipherText {
            msg: Base64EncodedText::from(cipher_text),
            auth_data: plain_text.auth_data.clone(),
        }
    }

    pub fn decrypt(&self, cipher_text: &AeadCipherText, decryption_direction: DecryptionDirection) -> AeadPlainText {
        let auth_data = &cipher_text.auth_data;

        let their_pk = match decryption_direction {
            DecryptionDirection::Straight => CryptoBoxPublicKey::from(&auth_data.sender_public_key),
            DecryptionDirection::Backward => CryptoBoxPublicKey::from(&auth_data.receiver_public_key),
        };
        let crypto_box = self.build_cha_cha_box(&their_pk);

        let msg_vec: Vec<u8> = cipher_text.msg.clone().into();
        let decrypted_plaintext: Vec<u8> = crypto_box
            .decrypt(
                &Nonce::from(&auth_data.nonce),
                Payload {
                    msg: msg_vec.as_bytes(),
                    aad: auth_data.associated_data.as_bytes(),
                },
            )
            .unwrap();

        AeadPlainText {
            msg: String::from_utf8(decrypted_plaintext).unwrap(),
            auth_data: cipher_text.auth_data.clone(),
        }
    }

    pub fn generate_nonce(&self) -> Base64EncodedText {
        let nonce: Nonce = ChaChaBox::generate_nonce(&mut CryptoBoxOsRng);
        Base64EncodedText::from(nonce.as_slice())
    }
}

pub enum DecryptionDirection {
    //Receiver decrypts message.
    // The message encrypted by sender and we use receiver's secret key and sender's public key to get a password
    Straight,
    //Sender gets back its encrypted message and wants to decrypt it.
    // In this case we use sender's secret key and receiver's public key to derive the encryption password
    Backward,
}

#[cfg(test)]
pub mod test {
    use crate::crypto::key_pair::{DecryptionDirection, KeyPair};
    use crate::crypto::keys::{AeadAuthData, AeadCipherText, AeadPlainText, KeyManager};

    #[test]
    fn single_person_encryption() {
        let password = "topSecret".to_string();

        let alice_km = KeyManager::generate();
        let cypher_text = alice_km
            .transport_key_pair
            .encrypt_string(password.clone(), alice_km.transport_key_pair.public_key());

        let plain_text = alice_km
            .transport_key_pair
            .decrypt(&cypher_text, DecryptionDirection::Straight);
        assert_eq!(password, plain_text.msg);

        let plain_text = alice_km
            .transport_key_pair
            .decrypt(&cypher_text, DecryptionDirection::Backward);
        assert_eq!(password, plain_text.msg);
    }

    #[test]
    fn straight_and_backward_decryption() {
        let alice_km = KeyManager::generate();
        let bob_km = KeyManager::generate();

        let plain_text = AeadPlainText {
            msg: "t0p$3cr3t".to_string(),
            auth_data: AeadAuthData {
                associated_data: "checksum".to_string(),
                sender_public_key: alice_km.transport_key_pair.public_key(),
                receiver_public_key: bob_km.transport_key_pair.public_key(),
                nonce: alice_km.transport_key_pair.generate_nonce(),
            },
        };
        let cipher_text: AeadCipherText = alice_km.transport_key_pair.encrypt(&plain_text);

        let decrypted_text = bob_km
            .transport_key_pair
            .decrypt(&cipher_text, DecryptionDirection::Straight);

        assert_eq!(plain_text, decrypted_text);

        let cipher_text = AeadCipherText {
            msg: cipher_text.msg,
            auth_data: AeadAuthData {
                associated_data: cipher_text.auth_data.associated_data,
                sender_public_key: cipher_text.auth_data.receiver_public_key,
                receiver_public_key: cipher_text.auth_data.sender_public_key,
                nonce: cipher_text.auth_data.nonce,
            },
        };

        let decrypted_text = bob_km
            .transport_key_pair
            .decrypt(&cipher_text, DecryptionDirection::Backward);

        assert_eq!(plain_text.msg, decrypted_text.msg);
    }
}
