use crypto_box::KEY_SIZE as KEY_SIZE_32_BYTES;

pub type Array256Bit = [u8; KEY_SIZE_32_BYTES];

/// Base64 encoding/decoding
pub mod base64 {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Eq, PartialEq, Hash, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Base64EncodedText {
        pub base64_text: String,
    }

    pub mod encoder {
        use crate::crypto::encoding::Array256Bit;

        use super::Base64EncodedText;

        impl From<Vec<u8>> for Base64EncodedText {
            fn from(data: Vec<u8>) -> Self {
                Self {
                    base64_text: base64::encode(&data),
                }
            }
        }

        impl From<&[u8]> for Base64EncodedText {
            fn from(data: &[u8]) -> Self {
                Self {
                    base64_text: base64::encode(data),
                }
            }
        }

        impl From<&Array256Bit> for Base64EncodedText {
            fn from(data: &Array256Bit) -> Self {
                Base64EncodedText::from(data.as_slice())
            }
        }
    }

    pub mod decoder {
        use crate::crypto::encoding::Array256Bit;
        use crate::errors::CoreError;

        use super::Base64EncodedText;

        impl TryFrom<&Base64EncodedText> for Vec<u8> {
            type Error = CoreError;

            fn try_from(base64: &Base64EncodedText) -> Result<Self, Self::Error> {
                let data = base64::decode(&base64.base64_text)?;
                Ok(data)
            }
        }

        impl TryFrom<&Base64EncodedText> for Array256Bit {
            type Error = CoreError;

            fn try_from(encoded: &Base64EncodedText) -> Result<Self, Self::Error> {
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
        use super::Base64EncodedText;

        #[test]
        fn from_vec() {
            let encoded = Base64EncodedText::from(vec![65, 65, 65]);
            let expected = Base64EncodedText {
                base64_text: "QUFB".to_string(),
            };
            assert_eq!(encoded, expected);
        }

        #[test]
        fn from_bytes() {
            let encoded = Base64EncodedText::from(b"AAA".as_slice());
            let expected = Base64EncodedText {
                base64_text: "QUFB".to_string(),
            };
            assert_eq!(encoded, expected);
        }
    }
}

pub mod serialized_key_manager {
    use crate::crypto::encoding::base64::Base64EncodedText;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Eq, PartialEq, Serialize, Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct SerializedKeyManager {
        pub dsa: SerializedDsaKeyPair,
        pub transport: SerializedTransportKeyPair,
    }

    #[derive(Debug, Eq, PartialEq, Serialize, Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct SerializedDsaKeyPair {
        pub key_pair: Base64EncodedText,
        pub public_key: Base64EncodedText,
    }

    #[derive(Debug, Eq, PartialEq, Serialize, Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct SerializedTransportKeyPair {
        pub secret_key: Base64EncodedText,
        pub public_key: Base64EncodedText,
    }

    pub mod encoder {
        use ed25519_dalek::ed25519::signature::Signature;

        use crate::crypto::key_pair::{DalekPublicKey, DalekSignature, DsaKeyPair, KeyPair, TransportDsaKeyPair};
        use crate::crypto::keys::KeyManager;

        use super::Base64EncodedText;
        use super::{SerializedDsaKeyPair, SerializedKeyManager, SerializedTransportKeyPair};

        // KeyManager -> SerializedKeyManager
        impl From<&KeyManager> for SerializedKeyManager {
            fn from(key_manager: &KeyManager) -> Self {
                Self {
                    dsa: SerializedDsaKeyPair::from(&key_manager.dsa),
                    transport: SerializedTransportKeyPair::from(&key_manager.transport_key_pair),
                }
            }
        }

        impl From<&TransportDsaKeyPair> for SerializedTransportKeyPair {
            fn from(transport: &TransportDsaKeyPair) -> Self {
                Self {
                    secret_key: transport.secret_key(),
                    public_key: transport.public_key(),
                }
            }
        }

        impl From<&DsaKeyPair> for SerializedDsaKeyPair {
            fn from(dsa: &DsaKeyPair) -> Self {
                Self {
                    key_pair: dsa.encode_key_pair(),
                    public_key: dsa.public_key(),
                }
            }
        }

        impl From<&DalekPublicKey> for Base64EncodedText {
            fn from(pk: &DalekPublicKey) -> Self {
                Base64EncodedText::from(&pk.to_bytes())
            }
        }

        impl From<&DalekSignature> for Base64EncodedText {
            fn from(sig: &DalekSignature) -> Self {
                Base64EncodedText::from(sig.as_bytes())
            }
        }
    }

    pub mod decoder {
        use crate::crypto::encoding::Array256Bit;
        use crate::crypto::key_pair::{
            CryptoBoxPublicKey, CryptoBoxSecretKey, DalekKeyPair, DalekPublicKey, DalekSignature, DsaKeyPair,
            TransportDsaKeyPair,
        };
        use crate::crypto::keys::KeyManager;
        use crate::errors::CoreError;

        use super::Base64EncodedText;
        use super::{SerializedDsaKeyPair, SerializedKeyManager, SerializedTransportKeyPair};

        impl TryFrom<&SerializedDsaKeyPair> for DsaKeyPair {
            type Error = CoreError;

            fn try_from(serialized_dsa: &SerializedDsaKeyPair) -> Result<Self, Self::Error> {
                let key_pair_vec: Vec<u8> = Vec::try_from(&serialized_dsa.key_pair)?;
                let key_pair = DalekKeyPair::from_bytes(key_pair_vec.as_slice())?;
                Ok(Self { key_pair })
            }
        }

        impl TryFrom<&Base64EncodedText> for DalekPublicKey {
            type Error = CoreError;

            fn try_from(base64_text: &Base64EncodedText) -> Result<Self, Self::Error> {
                let bytes = Array256Bit::try_from(base64_text)?;
                let pk = DalekPublicKey::from_bytes(&bytes)?;
                Ok(pk)
            }
        }

        impl TryFrom<&Base64EncodedText> for DalekSignature {
            type Error = CoreError;

            fn try_from(base64: &Base64EncodedText) -> Result<Self, Self::Error> {
                let bytes_vec: Vec<u8> = Vec::try_from(base64)?;
                let sign = DalekSignature::from_bytes(bytes_vec.as_slice())?;
                Ok(sign)
            }
        }

        impl TryFrom<&SerializedTransportKeyPair> for TransportDsaKeyPair {
            type Error = CoreError;

            fn try_from(serialized_transport: &SerializedTransportKeyPair) -> Result<Self, Self::Error> {
                let sk_bytes = Array256Bit::try_from(&serialized_transport.secret_key)?;
                let secret_key = CryptoBoxSecretKey::from(sk_bytes);
                let pk_bytes = Array256Bit::try_from(&serialized_transport.public_key)?;
                let public_key = CryptoBoxPublicKey::from(pk_bytes);

                let key_pair = Self { secret_key, public_key };

                Ok(key_pair)
            }
        }

        impl TryFrom<&SerializedKeyManager> for KeyManager {
            type Error = CoreError;

            fn try_from(serialized_km: &SerializedKeyManager) -> Result<Self, Self::Error> {
                let dsa = DsaKeyPair::try_from(&serialized_km.dsa)?;
                let transport_key_pair = TransportDsaKeyPair::try_from(&serialized_km.transport)?;
                let key_manager = Self {
                    dsa,
                    transport_key_pair,
                };

                Ok(key_manager)
            }
        }

        #[cfg(test)]
        pub mod test {
            use crate::crypto::encoding::base64::Base64EncodedText;
            use crate::crypto::key_pair::{DalekPublicKey, DalekSignature, KeyPair};
            use crate::crypto::keys::KeyManager;
            use crate::errors::CoreError;

            #[test]
            fn from_base64_to_dalek_public_key() -> Result<(), CoreError> {
                let km = KeyManager::generate();
                let pk_encoded = km.dsa.public_key();
                let pk = DalekPublicKey::try_from(&pk_encoded)?;
                assert_eq!(km.dsa.key_pair.public, pk);
                Ok(())
            }

            #[test]
            fn serialize_signature() -> Result<(), CoreError> {
                let km = KeyManager::generate();
                let serialized_sign = km.dsa.sign("text".to_string());
                let deserialized_sign = DalekSignature::try_from(&serialized_sign)?;
                let serialized_sign_2nd_time = Base64EncodedText::from(&deserialized_sign);

                assert_eq!(serialized_sign, serialized_sign_2nd_time);
                Ok(())
            }
        }
    }
}

pub mod cryptobox {
    pub mod decoder {
        use crypto_box::Nonce;

        use crate::crypto::encoding::base64::Base64EncodedText;
        use crate::crypto::encoding::Array256Bit;
        use crate::crypto::key_pair::{CryptoBoxPublicKey, CryptoBoxSecretKey};
        use crate::errors::CoreError;

        impl TryFrom<&Base64EncodedText> for CryptoBoxPublicKey {
            type Error = CoreError;

            fn try_from(encoded: &Base64EncodedText) -> Result<Self, Self::Error> {
                let byte_array = Array256Bit::try_from(encoded)?;
                Ok(CryptoBoxPublicKey::from(byte_array))
            }
        }

        impl TryFrom<&Base64EncodedText> for CryptoBoxSecretKey {
            type Error = CoreError;

            fn try_from(encoded: &Base64EncodedText) -> Result<Self, Self::Error> {
                let byte_array = Array256Bit::try_from(encoded)?;
                Ok(CryptoBoxSecretKey::from(byte_array))
            }
        }

        impl TryFrom<&Base64EncodedText> for Nonce {
            type Error = CoreError;

            fn try_from(encoded: &Base64EncodedText) -> Result<Self, Self::Error> {
                let vec = Vec::try_from(encoded)?;
                let byte_array: [u8; 24] = vec.as_slice().try_into()?;
                Ok(Nonce::from(byte_array))
            }
        }
    }
}
