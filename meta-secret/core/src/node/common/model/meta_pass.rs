use crate::crypto::utils::U64IdUrlEnc;
use wasm_bindgen::prelude::wasm_bindgen;

pub const SALT_LENGTH: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
//#[serde(transparent)]
#[wasm_bindgen(getter_with_clone)]
pub struct MetaPasswordId {
    pub id: U64IdUrlEnc,
    /// Human-readable name given to the password
    pub name: String,
}

#[wasm_bindgen]
impl MetaPasswordId {
    pub fn id(&self) -> String {
        self.id.text.base64_str()
    }

    pub fn build(name: &str) -> Self {
        Self {
            id: U64IdUrlEnc::from(name.to_string()),
            name: name.to_string(),
        }
    }
}
