use crypto_box::Nonce;
use crypto_box::{PublicKey as CryptoBoxPublicKey, KEY_SIZE};
use image::EncodableLayout;
use serde::{Deserialize, Serialize};

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

impl From<Base64EncodedText> for [u8; KEY_SIZE] {
    fn from(encoded: Base64EncodedText) -> Self {
        let bytes = base64::decode(&encoded.base64_text).unwrap();
        let bytes: [u8; KEY_SIZE] = bytes.as_slice().try_into().unwrap();
        bytes
    }
}

impl From<&Base64EncodedText> for Nonce {
    fn from(encoded: &Base64EncodedText) -> Self {
        let bytes = base64::decode(&encoded.base64_text).unwrap();
        let bytes: [u8; 24] = bytes.as_slice().try_into().unwrap();

        //use crypto_box::XNonce;
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
