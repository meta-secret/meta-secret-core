use age::secrecy::ExposeSecret;
use age::x25519::Identity;
use ed25519_dalek::{SecretKey, Signer, SigningKey};
use rand::rngs::OsRng as RandOsRng;
use rand::RngCore;

use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::keys::{DsaPk, DsaSk, TransportPk, TransportSk};
use crate::node::common::model::crypto::aead::{AeadCipherText, AeadPlainText};
use crate::node::common::model::crypto::channel::CommunicationChannel;
use crate::secret::shared_secret::PlainText;
use crate::CoreResult;

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
    pub secret_key: Identity,
}

impl TransportDsaKeyPair {
    pub fn sk(&self) -> TransportSk {
        let raw_sk = Base64Text::from(self.secret_key.to_string().expose_secret());
        TransportSk(raw_sk)
    }
}

impl KeyPair<TransportPk, TransportSk> for TransportDsaKeyPair {
    fn generate() -> Self {
        let secret_key = Identity::generate();
        Self { secret_key }
    }

    fn pk(&self) -> TransportPk {
        let base64 = Base64Text::from(self.secret_key.to_public().to_string());
        TransportPk::from(base64)
    }

    fn sk(&self) -> TransportSk {
        let raw_sk = Base64Text::from(self.secret_key.to_string().expose_secret());
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
            channel,
        };

        plain_text.encrypt()
    }
}

#[cfg(test)]
pub mod test {
    use crate::crypto::encoding::base64::Base64Text;
    use crate::crypto::key_pair::{DsaKeyPair, KeyPair, TransportDsaKeyPair};
    use crate::crypto::keys::fixture::KeyManagerFixture;
    use crate::node::common::model::crypto::aead::{AeadCipherText, AeadPlainText};
    use crate::node::common::model::crypto::channel::CommunicationChannel;
    use crate::node::common::model::IdString;
    use crate::secret::shared_secret::PlainText;

    #[test]
    fn test_dsa_key_pair_generate() {
        let key_pair = DsaKeyPair::generate();

        // Check that the generated key pair is valid
        assert!(!key_pair.key_pair.to_bytes().is_empty());

        // Verify that public and secret keys are generated correctly
        let pk = key_pair.pk();
        let sk = key_pair.sk();

        assert!(!pk.0.to_string().is_empty());
        assert!(!sk.0.to_string().is_empty());
    }

    #[test]
    fn test_dsa_key_pair_sign() {
        let key_pair = DsaKeyPair::generate();
        let text = "Test message to sign".to_string();

        let signature = key_pair.sign(text.clone());

        // Ensure signature is not empty
        assert!(!signature.to_string().is_empty());

        // Ensure different messages have different signatures
        let another_text = "Different message".to_string();
        let another_signature = key_pair.sign(another_text);

        assert_ne!(signature.to_string(), another_signature.to_string());
    }

    #[test]
    fn test_dsa_key_pair_encode() {
        let key_pair = DsaKeyPair::generate();

        let encoded = key_pair.encode_key_pair();

        // Ensure encoding is not empty
        assert!(!encoded.to_string().is_empty());
    }

    #[test]
    fn test_transport_dsa_key_pair_generate() {
        let key_pair = TransportDsaKeyPair::generate();

        // Verify that the transport key pair is valid
        let pk = key_pair.pk();
        let sk = key_pair.sk();

        // Just verify the device ID is not empty
        assert!(!pk.to_device_id().0.text.base64_str().is_empty());
        assert!(!sk.0.to_string().is_empty());
    }

    #[test]
    fn test_transport_key_pair_pk_sk_consistency() {
        let key_pair = TransportDsaKeyPair::generate();

        // Get pk and sk multiple times and ensure they are consistent
        let pk1 = key_pair.pk();
        let pk2 = key_pair.pk();
        let sk1 = key_pair.sk();
        let sk2 = key_pair.sk();

        // Compare IDs using id_str()
        assert_eq!(pk1.to_device_id().0.id_str(), pk2.to_device_id().0.id_str());
        assert_eq!(sk1.0.to_string(), sk2.0.to_string());
    }

    #[test]
    fn single_person_encryption() -> anyhow::Result<()> {
        let password = PlainText::from("topSecret");

        let fixture = KeyManagerFixture::generate();
        let alice_km = fixture.client;
        let cipher_text: AeadCipherText = alice_km
            .transport
            .encrypt_string(password.clone(), &alice_km.transport.pk())?;

        let plain_text = cipher_text.decrypt(&alice_km.transport.sk())?;
        assert_eq!(Base64Text::from(password), plain_text.msg);

        Ok(())
    }

    #[test]
    fn straight_and_backward_decryption() -> anyhow::Result<()> {
        let fixture = KeyManagerFixture::generate();
        let alice_km = fixture.client;
        let bob_km = fixture.client_b;

        let channel = CommunicationChannel::build(alice_km.transport.pk(), bob_km.transport.pk());

        let plain_text = {
            AeadPlainText {
                msg: Base64Text::from("t0p$3cr3t"),
                channel,
            }
        };

        let cipher_text = plain_text.encrypt()?;

        let decrypted_text = cipher_text.decrypt(&bob_km.transport.sk())?;
        assert_eq!(plain_text, decrypted_text);

        let decrypted_text = cipher_text.decrypt(&alice_km.transport.sk())?;
        assert_eq!(plain_text, decrypted_text);

        let cipher_text = AeadCipherText {
            msg: cipher_text.msg,
            channel: cipher_text.channel.inverse(),
        };

        let decrypted_text = cipher_text.decrypt(&bob_km.transport.sk())?;

        assert_eq!(plain_text.msg, decrypted_text.msg);

        Ok(())
    }

    #[test]
    fn third_party_decryption() -> anyhow::Result<()> {
        let fixture = KeyManagerFixture::generate();
        let alice_km = fixture.client;
        let bob_km = fixture.client_b;

        let cipher_text: AeadCipherText = alice_km
            .transport
            .encrypt_string(PlainText::from("secret"), &alice_km.transport.pk())?;

        let error_result = cipher_text.decrypt(&bob_km.transport.sk());
        let error = error_result.is_err();

        assert!(error);

        Ok(())
    }

    #[test]
    fn test_encrypt_string_functionality() -> anyhow::Result<()> {
        let alice_key_pair = TransportDsaKeyPair::generate();
        let bob_key_pair = TransportDsaKeyPair::generate();

        let plain_text = PlainText::from("Hello, Bob!");

        // Alice encrypts a message for Bob
        let cipher_text = alice_key_pair.encrypt_string(plain_text.clone(), &bob_key_pair.pk())?;

        // Ensure the cipher text has the correct channel
        let alice_device_id = alice_key_pair.pk().to_device_id();
        let sender_device_id = cipher_text.channel.sender().to_device_id();
        assert_eq!(
            alice_device_id.0.text.base64_str(),
            sender_device_id.0.text.base64_str()
        );

        let bob_device_id = bob_key_pair.pk().to_device_id();
        let receiver_device_id = cipher_text.channel.receiver().to_device_id();
        assert_eq!(
            bob_device_id.0.text.base64_str(),
            receiver_device_id.0.text.base64_str()
        );

        // Bob should be able to decrypt
        let decrypted = cipher_text.decrypt(&bob_key_pair.sk())?;

        // Verify the decrypted message matches the original
        assert_eq!(decrypted.msg, Base64Text::from(plain_text));

        Ok(())
    }

    #[test]
    fn test_different_keys_generate_different_outputs() {
        // Test that multiple key pairs generate different keys
        let key_pair1 = DsaKeyPair::generate();
        let key_pair2 = DsaKeyPair::generate();

        assert_ne!(key_pair1.pk().0.to_string(), key_pair2.pk().0.to_string());
        assert_ne!(key_pair1.sk().0.to_string(), key_pair2.sk().0.to_string());

        let t_key_pair1 = TransportDsaKeyPair::generate();
        let t_key_pair2 = TransportDsaKeyPair::generate();

        // Compare using the base64 string of the device ID's text
        assert_ne!(
            t_key_pair1.pk().to_device_id().0.text.base64_str(),
            t_key_pair2.pk().to_device_id().0.text.base64_str()
        );
        assert_ne!(
            t_key_pair1.sk().0.to_string(),
            t_key_pair2.sk().0.to_string()
        );
    }
}
