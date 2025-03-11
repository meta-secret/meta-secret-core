const KEY_SIZE_32_BYTES: usize = 32;

pub type Array256Bit = [u8; KEY_SIZE_32_BYTES];

/// Base64 encoding/decoding
pub mod base64 {
    extern crate base64;

    use std::fmt::Display;
    use wasm_bindgen::prelude::wasm_bindgen;

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[wasm_bindgen(getter_with_clone)]
    pub struct Base64Text(String);

    impl Display for Base64Text {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0.clone())
        }
    }

    impl Base64Text {
        pub fn base64_str(&self) -> String {
            self.0.clone()
        }
    }

    pub mod encoder {
        use crate::crypto::encoding::base64::Base64Text;
        use crate::crypto::encoding::Array256Bit;
        use crate::secret::shared_secret::PlainText;
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        use image::EncodableLayout;

        impl From<Vec<u8>> for Base64Text {
            fn from(data: Vec<u8>) -> Self {
                Base64Text::from(data.as_slice())
            }
        }

        impl From<&[u8]> for Base64Text {
            fn from(data: &[u8]) -> Self {
                Self(URL_SAFE_NO_PAD.encode(data))
            }
        }

        impl From<String> for Base64Text {
            fn from(data: String) -> Self {
                Base64Text::from(data.as_bytes())
            }
        }

        impl From<PlainText> for Base64Text {
            fn from(data: PlainText) -> Self {
                Base64Text::from(data.as_bytes())
            }
        }

        impl From<&str> for Base64Text {
            fn from(data: &str) -> Self {
                Base64Text::from(data.as_bytes())
            }
        }

        impl From<&Array256Bit> for Base64Text {
            fn from(data: &Array256Bit) -> Self {
                Base64Text::from(data.as_bytes())
            }
        }

        impl From<Array256Bit> for Base64Text {
            fn from(data: Array256Bit) -> Self {
                Base64Text::from(data.as_slice())
            }
        }

        #[cfg(test)]
        mod test {
            use crate::crypto::encoding::base64::Base64Text;
            use crate::secret::shared_secret::PlainText;

            #[test]
            fn plain_text_test() {
                let plain_text = PlainText::from("2bee~");
                let base64 = Base64Text::from(plain_text);
                assert_eq!(base64, Base64Text::from("2bee~"))
            }
        }
    }

    pub mod decoder {
        use crate::crypto::encoding::base64::Base64Text;
        use crate::crypto::encoding::Array256Bit;
        use crate::errors::CoreError;
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

        impl TryFrom<&Base64Text> for String {
            type Error = CoreError;

            fn try_from(base64: &Base64Text) -> Result<Self, Self::Error> {
                //decode base64 string
                let bytes_vec = Vec::try_from(base64)?;
                Ok(String::from_utf8(bytes_vec)?)
            }
        }

        impl TryFrom<&Base64Text> for Vec<u8> {
            type Error = CoreError;

            fn try_from(base64: &Base64Text) -> Result<Self, Self::Error> {
                let Base64Text(base64_text) = base64;
                let data = URL_SAFE_NO_PAD.decode(base64_text)?;
                Ok(data)
            }
        }

        impl TryFrom<&Base64Text> for Array256Bit {
            type Error = CoreError;

            fn try_from(encoded: &Base64Text) -> Result<Self, Self::Error> {
                //decode base64 string
                let bytes_vec = Vec::try_from(encoded)?;
                //try to cast an array to a fixed size array
                let byte_array: Array256Bit = bytes_vec.as_slice().try_into()?;
                Ok(byte_array)
            }
        }
    }

    #[cfg(test)]
    mod test {
        use crate::crypto::encoding::base64::Base64Text;
        use crate::crypto::encoding::Array256Bit;
        use crate::crypto::encoding::KEY_SIZE_32_BYTES;
        use crate::secret::shared_secret::PlainText;

        const TEST_STR: &str = "kjsfdbkjsfhdkjhsfdkjhsfdkjhksfdjhksjfdhksfd";
        const ENCODED_URL_SAFE_TEST_STR: &str =
            "a2pzZmRia2pzZmhka2poc2Zka2poc2Zka2poa3NmZGpoa3NqZmRoa3NmZA";

        #[test]
        fn from_vec() {
            let encoded = Base64Text::from(vec![65, 65, 65]);
            let expected = Base64Text("QUFB".to_string());
            assert_eq!(encoded, expected);
        }

        #[test]
        fn from_bytes() {
            let encoded = Base64Text::from(TEST_STR.as_bytes());
            let expected = Base64Text(ENCODED_URL_SAFE_TEST_STR.to_string());
            assert_eq!(encoded, expected);
        }

        #[test]
        fn from_string() {
            let encoded = Base64Text::from(String::from(TEST_STR));
            let expected = Base64Text(ENCODED_URL_SAFE_TEST_STR.to_string());
            assert_eq!(encoded, expected);
        }

        #[test]
        fn from_str() {
            let encoded = Base64Text::from(TEST_STR);
            let expected = Base64Text(ENCODED_URL_SAFE_TEST_STR.to_string());
            assert_eq!(encoded, expected);
        }

        #[test]
        fn from_plaintext() {
            let plain_text = PlainText::from(TEST_STR);
            let encoded = Base64Text::from(plain_text);
            let expected = Base64Text(ENCODED_URL_SAFE_TEST_STR.to_string());
            assert_eq!(encoded, expected);
        }

        #[test]
        fn from_array256bit() {
            let mut array = [0u8; KEY_SIZE_32_BYTES];
            for i in 0..array.len() {
                array[i] = i as u8;
            }

            let encoded = Base64Text::from(array);
            let encoded_ref = Base64Text::from(&array);

            // Both ways of encoding should produce the same result
            assert_eq!(encoded, encoded_ref);

            // Verify the actual encoding
            let expected = Base64Text("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8".to_string());
            assert_eq!(encoded, expected);
        }

        #[test]
        fn display_implementation() {
            let base64 = Base64Text("test_string".to_string());
            assert_eq!(format!("{}", base64), "test_string");
        }

        #[test]
        fn base64_str_method() {
            let base64 = Base64Text("test_string".to_string());
            assert_eq!(base64.base64_str(), "test_string");
        }

        #[test]
        fn decode_to_string() {
            let base64 = Base64Text(ENCODED_URL_SAFE_TEST_STR.to_string());
            let decoded = String::try_from(&base64).unwrap();
            assert_eq!(decoded, TEST_STR);
        }

        #[test]
        fn decode_to_vec() {
            let base64 = Base64Text(ENCODED_URL_SAFE_TEST_STR.to_string());
            let decoded: Vec<u8> = Vec::try_from(&base64).unwrap();
            assert_eq!(decoded, TEST_STR.as_bytes());
        }

        #[test]
        fn roundtrip_array256bit() {
            let mut original = [0u8; KEY_SIZE_32_BYTES];
            for i in 0..original.len() {
                original[i] = i as u8;
            }

            let encoded = Base64Text::from(&original);
            let decoded = Array256Bit::try_from(&encoded).unwrap();

            assert_eq!(original, decoded);
        }

        #[test]
        fn decode_invalid_base64() {
            let invalid_base64 = Base64Text("@$%^&*(".to_string());
            let result: Result<Vec<u8>, _> = Vec::try_from(&invalid_base64);
            assert!(result.is_err(), "Should fail with invalid base64 input");
        }

        #[test]
        fn decode_wrong_size_array() {
            // Create a Base64Text that encodes a shorter array than Array256Bit
            let short_data = [1, 2, 3, 4, 5];
            let encoded = Base64Text::from(&short_data[..]);

            // Trying to decode into Array256Bit should fail
            let result = Array256Bit::try_from(&encoded);
            assert!(
                result.is_err(),
                "Should fail when decoding to wrong size array"
            );
        }
    }
}

pub mod serialized_key_manager {

    pub mod encoder {
        use crate::crypto::encoding::base64::Base64Text;
        use crate::crypto::key_pair::{
            DalekPublicKey, DalekSignature, DsaKeyPair, KeyPair, TransportDsaKeyPair,
        };
        use crate::crypto::keys::{
            KeyManager, SecretBox, SerializedDsaKeyPair, SerializedTransportKeyPair,
        };

        // KeyManager -> SecretBox
        impl From<&KeyManager> for SecretBox {
            fn from(key_manager: &KeyManager) -> Self {
                Self {
                    dsa: SerializedDsaKeyPair::from(&key_manager.dsa),
                    transport: SerializedTransportKeyPair::from(&key_manager.transport),
                }
            }
        }

        impl From<&TransportDsaKeyPair> for SerializedTransportKeyPair {
            fn from(transport: &TransportDsaKeyPair) -> Self {
                Self {
                    sk: transport.sk(),
                    pk: transport.pk(),
                }
            }
        }

        impl From<&DsaKeyPair> for SerializedDsaKeyPair {
            fn from(dsa: &DsaKeyPair) -> Self {
                Self {
                    key_pair: dsa.encode_key_pair(),
                    public_key: dsa.pk(),
                }
            }
        }

        impl From<&DalekPublicKey> for Base64Text {
            fn from(pk: &DalekPublicKey) -> Self {
                Base64Text::from(pk.as_bytes())
            }
        }

        impl From<&DalekSignature> for Base64Text {
            fn from(sig: &DalekSignature) -> Self {
                Base64Text::from(sig.to_vec())
            }
        }
    }

    pub mod decoder {
        use crate::crypto::encoding::base64::Base64Text;
        use crate::crypto::encoding::Array256Bit;
        use crate::crypto::key_pair::{
            DalekKeyPair, DalekPublicKey, DalekSignature, DsaKeyPair, TransportDsaKeyPair,
        };
        use crate::crypto::keys::{
            KeyManager, SecretBox, SerializedDsaKeyPair, SerializedTransportKeyPair,
        };
        use crate::errors::CoreError;
        use age::x25519::Identity;
        use std::str::FromStr;

        impl TryFrom<&SerializedDsaKeyPair> for DsaKeyPair {
            type Error = CoreError;

            fn try_from(serialized_dsa: &SerializedDsaKeyPair) -> Result<Self, Self::Error> {
                let key_pair_vec = Array256Bit::try_from(&serialized_dsa.key_pair)?;
                let key_pair = DalekKeyPair::from_bytes(&key_pair_vec);
                Ok(Self { key_pair })
            }
        }

        impl TryFrom<&Base64Text> for DalekPublicKey {
            type Error = CoreError;

            fn try_from(base64_text: &Base64Text) -> Result<Self, Self::Error> {
                let bytes = Array256Bit::try_from(base64_text)?;
                let pk = DalekPublicKey::from_bytes(&bytes)?;
                Ok(pk)
            }
        }

        impl TryFrom<&Base64Text> for DalekSignature {
            type Error = CoreError;

            fn try_from(base64: &Base64Text) -> Result<Self, Self::Error> {
                let bytes_vec: Vec<u8> = Vec::try_from(base64)?;
                let sign = DalekSignature::from_slice(bytes_vec.as_slice())?;
                Ok(sign)
            }
        }

        impl TryFrom<&SerializedTransportKeyPair> for TransportDsaKeyPair {
            type Error = CoreError;

            fn try_from(
                serialized_transport: &SerializedTransportKeyPair,
            ) -> Result<Self, Self::Error> {
                let sk_bytes = String::try_from(&serialized_transport.sk.0)?;
                let sk_res = Identity::from_str(sk_bytes.as_str());

                match sk_res {
                    Ok(secret_key) => Ok(Self { secret_key }),
                    Err(err_str) => Err(CoreError::InvalidSizeEncryptionError {
                        err_msg: err_str.to_string(),
                    }),
                }
            }
        }

        impl TryFrom<&SecretBox> for KeyManager {
            type Error = CoreError;

            fn try_from(serialized_km: &SecretBox) -> Result<Self, Self::Error> {
                let dsa = DsaKeyPair::try_from(&serialized_km.dsa)?;
                let transport_key_pair = TransportDsaKeyPair::try_from(&serialized_km.transport)?;
                let key_manager = Self {
                    dsa,
                    transport: transport_key_pair,
                };

                Ok(key_manager)
            }
        }

        #[cfg(test)]
        pub mod test {
            use crate::crypto::encoding::base64::Base64Text;
            use crate::crypto::key_pair::{DalekPublicKey, DalekSignature, KeyPair};
            use crate::crypto::key_pair::{DsaKeyPair, TransportDsaKeyPair};
            use crate::crypto::keys::{
                KeyManager, SecretBox, SerializedDsaKeyPair, SerializedTransportKeyPair,
            };
            use crate::CoreResult;
            use ed25519_dalek::Verifier;

            #[test]
            fn from_base64_to_dalek_public_key() -> CoreResult<()> {
                let km = KeyManager::generate();
                let pk_encoded = km.dsa.pk();
                let pk = DalekPublicKey::try_from(&pk_encoded.0)?;
                assert_eq!(km.dsa.key_pair.verifying_key(), pk);
                Ok(())
            }

            #[test]
            fn serialize_signature() -> CoreResult<()> {
                let km = KeyManager::generate();
                let serialized_sign = km.dsa.sign("text".to_string());
                let deserialized_sign = DalekSignature::try_from(&serialized_sign)?;
                let serialized_sign_2nd_time = Base64Text::from(&deserialized_sign);

                assert_eq!(serialized_sign, serialized_sign_2nd_time);
                Ok(())
            }

            #[test]
            fn key_manager_serialization_roundtrip() -> CoreResult<()> {
                // Generate a key manager
                let original_km = KeyManager::generate();

                // Serialize to SecretBox
                let secret_box = SecretBox::from(&original_km);

                // Deserialize back to KeyManager
                let deserialized_km = KeyManager::try_from(&secret_box)?;

                // Verify public keys match (full equality can't be tested due to private key comparison)
                assert_eq!(original_km.dsa.pk(), deserialized_km.dsa.pk());
                assert_eq!(original_km.transport.pk(), deserialized_km.transport.pk());

                Ok(())
            }

            #[test]
            fn dsa_key_pair_serialization_roundtrip() -> CoreResult<()> {
                // Generate a DSA key pair
                let original_dsa = DsaKeyPair::generate();

                // Serialize
                let serialized = SerializedDsaKeyPair::from(&original_dsa);

                // Deserialize
                let deserialized = DsaKeyPair::try_from(&serialized)?;

                // Verify the public key matches
                assert_eq!(original_dsa.pk(), deserialized.pk());

                // Test signing with both to verify functionality
                let test_msg = "test message";
                let signature1 = original_dsa.sign(test_msg.to_string());
                let signature2 = deserialized.sign(test_msg.to_string());

                // Different signatures (randomized) but both should verify correctly
                // Convert Base64Text signatures to DalekSignature for verification
                let dalek_sig1 = DalekSignature::try_from(&signature1)?;
                let dalek_sig2 = DalekSignature::try_from(&signature2)?;

                // Verify signatures using the verifying_key
                original_dsa
                    .key_pair
                    .verifying_key()
                    .verify(test_msg.as_bytes(), &dalek_sig2)?;
                deserialized
                    .key_pair
                    .verifying_key()
                    .verify(test_msg.as_bytes(), &dalek_sig1)?;

                Ok(())
            }

            #[test]
            fn transport_key_pair_serialization_roundtrip() -> CoreResult<()> {
                // Generate a transport key pair
                let original_transport = TransportDsaKeyPair::generate();

                // Serialize
                let serialized = SerializedTransportKeyPair::from(&original_transport);

                // Deserialize
                let deserialized = TransportDsaKeyPair::try_from(&serialized)?;

                // Verify the public key matches
                assert_eq!(original_transport.pk(), deserialized.pk());

                Ok(())
            }

            #[test]
            fn invalid_dalek_public_key_conversion() {
                // Create an invalid Base64Text for conversion to DalekPublicKey
                let invalid_base64 = Base64Text::from("too_short");
                let result = DalekPublicKey::try_from(&invalid_base64);
                assert!(result.is_err(), "Should fail with invalid public key data");
            }

            #[test]
            fn invalid_dalek_signature_conversion() {
                // Create an invalid Base64Text for conversion to DalekSignature
                let invalid_base64 = Base64Text::from("invalid_signature_data");
                let result = DalekSignature::try_from(&invalid_base64);
                assert!(result.is_err(), "Should fail with invalid signature data");
            }
        }
    }
}
