use crate::crypto::utils;
use rand::{distributions::Alphanumeric, Rng};

use crate::models::MetaPasswordId;

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
        let mut id_str = name.clone();
        id_str.push('-');
        id_str.push_str(salt.as_str());

        Self {
            id: utils::generate_uuid_b64_url_enc(id_str),
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
        assert_eq!(pass_id.id, "CHKANX39xaMXfhe3Qkx9-w".to_string())
    }
}
