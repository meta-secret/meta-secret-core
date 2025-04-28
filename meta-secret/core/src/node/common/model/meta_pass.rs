use crate::crypto::utils::U64IdUrlEnc;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;
use secrecy::{SecretString, ExposeSecret};

pub const SALT_LENGTH: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct MetaPasswordId {
    pub id: U64IdUrlEnc,
    /// Human-readable name given to the password
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct PlainPassInfo {
    pub pass_id: MetaPasswordId,
    pub pass: String,
}

#[derive(Clone, Debug)]
pub struct SecurePassInfo {
    pub pass_id: MetaPasswordId,
    pub pass: SecretString,
}

impl From<PlainPassInfo> for SecurePassInfo {
    fn from(plain: PlainPassInfo) -> Self {
        Self {
            pass_id: plain.pass_id,
            pass: SecretString::new(plain.pass.into()),
        }
    }
}

impl SecurePassInfo {
    pub fn new(pass: SecretString, pass_name: String) -> Self {
        let pass_id = MetaPasswordId::build(&pass_name);
        Self { pass_id, pass }
    }
    
    pub fn to_plain(&self) -> PlainPassInfo {
        PlainPassInfo {
            pass_id: self.pass_id.clone(),
            pass: ExposeSecret::expose_secret(&self.pass).to_string(),
        }
    }
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

#[wasm_bindgen]
impl PlainPassInfo {
    #[wasm_bindgen(constructor)]
    pub fn new(pass_id_str: &str, pass: &str) -> Self {
        let pass_id = MetaPasswordId::build(pass_id_str);
        Self {
            pass_id,
            pass: pass.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meta_password_id_build() {
        let name = "Test Password";
        let password_id = MetaPasswordId::build(name);

        // Verify the name is preserved
        assert_eq!(password_id.name, name);

        // Verify an id was generated
        assert!(!password_id.id().is_empty());

        // Verify the id is deterministic (same name produces same id)
        let password_id2 = MetaPasswordId::build(name);
        assert_eq!(password_id.id(), password_id2.id());
    }

    #[test]
    fn test_meta_password_id_different_names() {
        let password_id1 = MetaPasswordId::build("Password 1");
        let password_id2 = MetaPasswordId::build("Password 2");

        // Different names should produce different ids
        assert_ne!(password_id1.id(), password_id2.id());
    }

    #[test]
    fn test_meta_password_id_cloning() {
        let original = MetaPasswordId::build("Original Password");
        let cloned = original.clone();

        // Cloning should produce an equal object
        assert_eq!(original, cloned);
        assert_eq!(original.id(), cloned.id());
        assert_eq!(original.name, cloned.name);
    }

    #[test]
    fn test_meta_password_id_equality() {
        let password1 = MetaPasswordId::build("Test Password");
        let password2 = MetaPasswordId::build("Test Password");
        let password3 = MetaPasswordId::build("Different Password");

        // Same name should create equal objects
        assert_eq!(password1, password2);

        // Different names should create non-equal objects
        assert_ne!(password1, password3);
    }
}
