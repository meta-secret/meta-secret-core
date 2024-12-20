use crate::crypto::encoding::base64::Base64Text;
use crate::node::common::model::IdString;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptorFqdn, ObjectDescriptorId};
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
#[wasm_bindgen(getter_with_clone)]
pub struct U64IdUrlEnc {
    pub text: Base64Text
}

impl U64IdUrlEnc {
    pub fn take(&self, n: usize) -> String {
        self.text.base64_str()
            .chars()
            .take(n)
            .collect::<String>()
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
    fn id_str(&self) -> String {
        self.text.base64_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct UuidUrlEnc {
    text: Base64Text
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
    fn id_str(&self) -> String {
        self.text.base64_str()
    }
}

pub trait NextId {
    fn next_id(self) -> ObjectDescriptorId;
}

impl NextId for ObjectDescriptorFqdn {
    fn next_id(self) -> ObjectDescriptorId {
        ObjectDescriptorId {
            fqdn: self.clone(),
            id: 0,
        }
    }
}

impl NextId for ObjectDescriptorId {
    fn next_id(self) -> ObjectDescriptorId {
        ObjectDescriptorId {
            id: self.id + 1,
            ..self
        }
    }
}
