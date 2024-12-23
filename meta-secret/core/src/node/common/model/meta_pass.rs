use crate::crypto::utils::U64IdUrlEnc;
use rand::distributions::Alphanumeric;
use rand::Rng;
use wasm_bindgen::prelude::wasm_bindgen;

pub const SALT_LENGTH: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct MetaPasswordId {
    /// SHA256 hash of a salt
    pub id: U64IdUrlEnc,
    /// Random String up to 30 characters, must be unique
    pub salt: String,
    /// Human-readable name given to the password
    pub name: String,
}

#[wasm_bindgen]
impl MetaPasswordId {
    pub fn id(&self) -> String {
        self.id.text.base64_str()
    }

    pub fn generate(name: String) -> Self {
        let salt: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(SALT_LENGTH)
            .map(char::from)
            .collect();
        MetaPasswordId::build(name, salt)
    }

    pub fn build(name: String, salt: String) -> Self {
        let mut id_str = name.clone();
        id_str.push('-');
        id_str.push_str(salt.as_str());

        Self {
            id: U64IdUrlEnc::from(id_str),
            salt,
            name,
        }
    }
}
