const KEY_SIZE_32_BYTES: usize = 32;

pub type Array256Bit = [u8; KEY_SIZE_32_BYTES];
pub type Base64String = String;

/// Base64 encoding/decoding
pub mod base64 {
    extern crate base64;

    use crate::crypto::encoding::Base64String;
    use std::fmt::Display;
    use wasm_bindgen::prelude::wasm_bindgen;

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[wasm_bindgen(getter_with_clone)]
    pub struct Base64Text(Base64String);

    impl Display for Base64Text {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0.clone())
        }
    }

    impl Base64Text {
        pub fn base64_str(&self) -> Base64String {
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
            use crate::crypto::keys::KeyManager;
            use crate::CoreResult;

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
        }
    }
}
