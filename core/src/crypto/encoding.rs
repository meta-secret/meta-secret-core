use crate::crypto::key_pair::{CryptoBoxPublicKey, CryptoBoxSecretKey};
use crypto_box::Nonce;
use crypto_box::KEY_SIZE;
use image::EncodableLayout;
use serde::{Deserialize, Serialize};

pub mod serialized_key_manager {
    use crate::crypto::encoding::Base64EncodedText;
    use crate::crypto::key_pair::TransportDsaKeyPair;
    use crate::crypto::key_pair::{CryptoBoxPublicKey, CryptoBoxSecretKey, DalekKeyPair};
    use crate::crypto::key_pair::{DsaKeyPair, KeyPair};
    use crate::crypto::keys::KeyManager;
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

    //SerializedKeyManager -> KeyManager
    impl From<&SerializedDsaKeyPair> for DsaKeyPair {
        fn from(serialized_dsa: &SerializedDsaKeyPair) -> Self {
            let key_pair_vec: Vec<u8> = base64::decode(&serialized_dsa.key_pair.base64_text).unwrap();
            let key_pair = DalekKeyPair::from_bytes(key_pair_vec.as_slice()).unwrap();
            Self { key_pair }
        }
    }

    impl From<&SerializedTransportKeyPair> for TransportDsaKeyPair {
        fn from(serialized_transport: &SerializedTransportKeyPair) -> Self {
            Self {
                secret_key: CryptoBoxSecretKey::from(&serialized_transport.secret_key),
                public_key: CryptoBoxPublicKey::from(&serialized_transport.public_key),
            }
        }
    }

    impl From<&SerializedKeyManager> for KeyManager {
        fn from(serialized_km: &SerializedKeyManager) -> Self {
            Self {
                dsa: DsaKeyPair::from(&serialized_km.dsa),
                transport_key_pair: TransportDsaKeyPair::from(&serialized_km.transport),
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Base64EncodedText {
    pub base64_text: String,
}

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

impl From<&[u8; KEY_SIZE]> for Base64EncodedText {
    fn from(data: &[u8; KEY_SIZE]) -> Self {
        Base64EncodedText::from(data.as_bytes())
    }
}

impl From<&Base64EncodedText> for CryptoBoxPublicKey {
    fn from(encoded: &Base64EncodedText) -> Self {
        let bytes = base64::decode(&encoded.base64_text).unwrap();
        let bytes: [u8; KEY_SIZE] = bytes.as_slice().try_into().unwrap();

        CryptoBoxPublicKey::from(bytes)
    }
}

impl From<&Base64EncodedText> for CryptoBoxSecretKey {
    fn from(encoded: &Base64EncodedText) -> Self {
        let bytes = base64::decode(&encoded.base64_text).unwrap();
        let bytes: [u8; KEY_SIZE] = bytes.as_slice().try_into().unwrap();

        CryptoBoxSecretKey::from(bytes)
    }
}

impl From<Base64EncodedText> for [u8; KEY_SIZE] {
    fn from(encoded: Base64EncodedText) -> Self {
        let bytes_vec = base64::decode(&encoded.base64_text).unwrap();
        let bytes: [u8; KEY_SIZE] = bytes_vec.as_slice().try_into().unwrap();
        bytes
    }
}

impl From<&Base64EncodedText> for Nonce {
    fn from(encoded: &Base64EncodedText) -> Self {
        let bytes = base64::decode(&encoded.base64_text).unwrap();
        let bytes: [u8; 24] = bytes.as_slice().try_into().unwrap();
        Nonce::from(bytes)
    }
}

impl From<Base64EncodedText> for Vec<u8> {
    fn from(data: Base64EncodedText) -> Self {
        base64::decode(data.base64_text).unwrap()
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::encoding::Base64EncodedText;
    use image::EncodableLayout;

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
        let encoded = Base64EncodedText::from(b"AAA".as_bytes());
        let expected = Base64EncodedText {
            base64_text: "QUFB".to_string(),
        };
        assert_eq!(encoded, expected);
    }
}
