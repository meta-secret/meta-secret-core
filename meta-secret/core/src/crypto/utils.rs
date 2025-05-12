use crate::crypto::encoding::base64::Base64Text;
use crate::node::common::model::IdString;
use derive_more::From;
use rand::rngs::OsRng;
use rand::TryRngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wasm_bindgen::prelude::wasm_bindgen;

const SEED_LENGTH: usize = 64;

pub fn generate_hash() -> String {
    // Use a cryptographically secure RNG to generate random bytes
    let mut seed_bytes = [0u8; SEED_LENGTH];
    OsRng
        .try_fill_bytes(&mut seed_bytes)
        .expect("Failed to get random bytes from OS");

    // Convert bytes directly to a hex string for simplicity
    let seed = hex::encode(seed_bytes);

    // Hash the seed with SHA-256
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());

    hex::encode(hasher.finalize())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(transparent)]
#[wasm_bindgen(getter_with_clone)]
pub struct Id48bit {
    pub text: String,
}

impl Id48bit {
    pub fn generate() -> Self {
        let mut random_bytes = [0u8; 8];
        OsRng
            .try_fill_bytes(&mut random_bytes)
            .expect("Failed to get random bytes from OS");

        // Convert to u64 and mask to 48 bits
        let random_u64 = u64::from_ne_bytes(random_bytes) & 0xFFFFFFFFFFFF;

        let hex_num = Self::hex(random_u64);
        Self { text: hex_num }
    }

    pub fn take(&self, n: usize) -> String {
        self.text.chars().take(n).collect::<String>()
    }

    fn hex(n: u64) -> String {
        let hex_string = format!("{:x}", n);

        // Calculate the length of each part (rounded down)
        let part_length = hex_string.len() / 3;

        // Split the string into four parts using slicing
        let (part1, rest) = hex_string.split_at(part_length);
        let (part2, part3) = rest.split_at(part_length);

        format!("{}-{}-{}", part1, part2, part3)
    }
}

impl IdString for Id48bit {
    fn id_str(self) -> String {
        self.text
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(transparent)]
#[wasm_bindgen(getter_with_clone)]
pub struct U64IdUrlEnc {
    pub text: Base64Text,
}

impl U64IdUrlEnc {
    pub fn take(&self, n: usize) -> String {
        self.text.base64_str().chars().take(n).collect::<String>()
    }
}

impl From<String> for U64IdUrlEnc {
    fn from(value: String) -> Self {
        let hash = Sha256::digest(value.as_bytes());
        let val = &hash.as_slice()[..8];
        let text = Base64Text::from(val);
        Self { text }
    }
}

impl IdString for U64IdUrlEnc {
    fn id_str(self) -> String {
        self.text.base64_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct UuidUrlEnc {
    text: Base64Text,
}

impl UuidUrlEnc {
    pub fn generate() -> UuidUrlEnc {
        // Use a cryptographically secure RNG for UUID generation
        let mut rng_bytes = [0u8; 16];
        OsRng
            .try_fill_bytes(&mut rng_bytes)
            .expect("Failed to get random bytes from OS");

        let uuid = Uuid::from_bytes(rng_bytes);
        let uuid_bytes = uuid.as_bytes().as_slice();
        let text = Base64Text::from(uuid_bytes);

        UuidUrlEnc { text }
    }
}

impl From<String> for UuidUrlEnc {
    fn from(value: String) -> Self {
        let hash = Sha256::digest(value.as_bytes());
        let uuid = Uuid::from_slice(&hash.as_slice()[..16]).unwrap();
        let text = Base64Text::from(uuid.as_bytes().as_slice());
        Self { text }
    }
}

impl IdString for UuidUrlEnc {
    fn id_str(self) -> String {
        self.text.base64_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_hash() {
        let hash1 = generate_hash();
        let hash2 = generate_hash();

        // Verify hash has correct length (32 bytes as hex = 64 chars)
        assert_eq!(hash1.len(), 64);

        // Verify uniqueness
        assert_ne!(hash1, hash2);

        // Verify it's a valid hex string
        assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_id48bit_generate() {
        let id1 = Id48bit::generate();
        let id2 = Id48bit::generate();

        // Verify uniqueness
        assert_ne!(id1.text, id2.text);

        // Verify format (should have two hyphens)
        assert_eq!(id1.text.matches('-').count(), 2);

        // Verify all parts (except hyphens) are hexadecimal
        let parts: Vec<&str> = id1.text.split('-').collect();
        assert_eq!(parts.len(), 3);
        for part in parts {
            assert!(!part.is_empty());
            assert!(part.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn test_id48bit_take() {
        let id = Id48bit {
            text: "abc-def-ghi".to_string(),
        };

        assert_eq!(id.take(3), "abc");
        assert_eq!(id.take(5), "abc-d");
        assert_eq!(id.take(0), "");
        assert_eq!(id.take(11), "abc-def-ghi");
        assert_eq!(id.take(20), "abc-def-ghi"); // Should not panic
    }

    #[test]
    fn test_id48bit_hex() {
        assert_eq!(Id48bit::hex(0), "--0");
        assert_eq!(Id48bit::hex(255), "--ff");
        assert_eq!(Id48bit::hex(1099511627775), "fff-fff-ffff"); // 2^40 - 1
    }

    #[test]
    fn test_id48bit_id_str() {
        let id = Id48bit {
            text: "abc-def-ghi".to_string(),
        };
        assert_eq!(id.id_str(), "abc-def-ghi");
    }

    #[test]
    fn test_u64idurlenc_from_string() {
        let id1 = U64IdUrlEnc::from("test1".to_string());
        let id2 = U64IdUrlEnc::from("test2".to_string());
        let id1_dupe = U64IdUrlEnc::from("test1".to_string());

        // Should be deterministic
        assert_eq!(id1.text.base64_str(), id1_dupe.text.base64_str());

        // Different input = different output
        assert_ne!(id1.text.base64_str(), id2.text.base64_str());
    }

    #[test]
    fn test_u64idurlenc_take() {
        let id = U64IdUrlEnc::from("test".to_string());
        let full = id.text.base64_str();

        assert_eq!(id.take(3), full.chars().take(3).collect::<String>());
        assert_eq!(id.take(0), "");
        assert_eq!(id.take(100), full); // Should not panic
    }

    #[test]
    fn test_u64idurlenc_id_str() {
        let id = U64IdUrlEnc::from("test".to_string());
        let expected = id.text.base64_str();
        assert_eq!(id.id_str(), expected);
    }

    #[test]
    fn test_uuidurlenc_generate() {
        let id1 = UuidUrlEnc::generate();
        let id2 = UuidUrlEnc::generate();

        // Verify uniqueness
        assert_ne!(id1.text.base64_str(), id2.text.base64_str());
    }

    #[test]
    fn test_uuidurlenc_from_string() {
        let id1 = UuidUrlEnc::from("test1".to_string());
        let id2 = UuidUrlEnc::from("test2".to_string());
        let id1_dupe = UuidUrlEnc::from("test1".to_string());

        // Should be deterministic
        assert_eq!(id1.text.base64_str(), id1_dupe.text.base64_str());

        // Different input = different output
        assert_ne!(id1.text.base64_str(), id2.text.base64_str());
    }

    #[test]
    fn test_uuidurlenc_id_str() {
        let id = UuidUrlEnc::from("test".to_string());
        let expected = id.text.base64_str();
        assert_eq!(id.id_str(), expected);
    }
}
