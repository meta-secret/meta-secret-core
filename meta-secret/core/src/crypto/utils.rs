use crate::crypto::encoding::base64::Base64Text;
use crate::node::common::model::IdString;
use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wasm_bindgen::prelude::wasm_bindgen;

const SEED_LENGTH: usize = 64;

pub fn generate_hash() -> String {
    let seed: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(SEED_LENGTH)
        .map(char::from)
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());

    hex::encode(hasher.finalize())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(transparent)]
#[wasm_bindgen(getter_with_clone)]
pub struct Id48bit {
    pub text: String,
}

impl Id48bit {
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();

        let random_u64: u64 = rng.gen::<u64>() & 0xFFFFFFFFFFFF;

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
        let uuid = Uuid::new_v4();
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
