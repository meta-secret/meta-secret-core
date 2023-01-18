use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha256};
use crate::models::{MetaPasswordId};

const SALT_LENGTH: usize = 8;

impl MetaPasswordId {
    pub fn generate(name: String) -> Self {
        let salt: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(SALT_LENGTH)
            .map(char::from)
            .collect();
        MetaPasswordId::build(name, salt)
    }

    pub fn build(name: String, salt: String) -> Self {
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn meta_password_id() {
        let pass_id = MetaPasswordId::build("test".to_string(), "salt".to_string());
        assert_eq!(
            pass_id.id,
            "087280357dfdc5a3177e17b7424c7dfb1eab2d08ba3bedeb243dc51d5c18dc88".to_string()
        )
    }
}
