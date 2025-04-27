use crate::crypto::utils::U64IdUrlEnc;
use serde::{Deserialize, Serialize, Serializer, Deserializer};
use wasm_bindgen::prelude::wasm_bindgen;
use secrecy::{SecretString, ExposeSecret};

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

#[derive(Clone, Debug)]
pub struct PassInfo {
    pub pass_id: MetaPasswordId,
    pub pass: SecretString,
}

impl PartialEq for PassInfo {
    fn eq(&self, other: &Self) -> bool {
        self.pass_id == other.pass_id && 
        ExposeSecret::expose_secret(&self.pass) == ExposeSecret::expose_secret(&other.pass)
    }
}

impl Eq for PassInfo {}

impl Serialize for PassInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("PassInfo", 2)?;
        state.serialize_field("pass_id", &self.pass_id)?;
        state.serialize_field("pass", ExposeSecret::expose_secret(&self.pass))?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for PassInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct PassInfoHelper {
            pass_id: MetaPasswordId,
            pass: String,
        }
        
        let helper = PassInfoHelper::deserialize(deserializer)?;
        
        Ok(PassInfo {
            pass_id: helper.pass_id,
            pass: SecretString::new(helper.pass.into()),
        })
    }
}

impl PassInfo {
    pub fn new(pass: SecretString, pass_name: String) -> Self {
        let pass_id = MetaPasswordId::build(&pass_name);
        Self { pass_id, pass }
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
