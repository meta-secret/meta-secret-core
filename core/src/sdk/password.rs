use crate::sdk::vault::VaultDoc;
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaPasswordId {
    // SHA256 hash of salt
    pub id: String,
    // Random String up to 30 characters, must be unique
    pub salt: String,
    // human readable name given to the password
    pub name: String,
}

impl MetaPasswordId {
    pub fn generate(name: String) -> Self {
        let salt: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        MetaPasswordId::new(name, salt)
    }

    pub fn new(name: String, salt: String) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(name.as_bytes());
        hasher.update("-".as_bytes());
        hasher.update(salt.as_bytes());

        let hash_bytes = hex::encode(hasher.finalize());

        Self {
            id: hash_bytes,
            salt,
            name,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaPasswordDoc {
    pub id: MetaPasswordId,
    //We need to keep the entire vault here,
    // because the vault can be changed (new members can appear some members can be deleted),
    // then we won't b e able to restore  the password if we'd have different members than in original vault
    pub vault: VaultDoc,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn meta_password_id() {
        let pass_id = MetaPasswordId::new("test".to_string(), "salt".to_string());
        assert_eq!(
            pass_id.id,
            "087280357dfdc5a3177e17b7424c7dfb1eab2d08ba3bedeb243dc51d5c18dc88".to_string()
        )
    }
}
