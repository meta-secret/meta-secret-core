use crate::models::Base64EncodedText;
use base64::alphabet::URL_SAFE;
use base64::engine::fast_portable::{FastPortable, NO_PAD};
use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha256};
use uuid::Uuid;

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

/// Generate random uuid encoded with base64 url encoding
pub fn rand_uuid_b64_url_enc() -> Base64EncodedText {
    let uuid = Uuid::new_v4();
    let uuid_bytes = uuid.as_bytes();
    Base64EncodedText::from(uuid_bytes.as_slice())
}

/// Convert a string to a base64 url encoded uuid
pub fn to_id(str: String) -> Base64EncodedText {
    let hash = Sha256::digest(str.as_bytes());
    let uuid = Uuid::from_slice(&hash.as_slice()[..16]).unwrap();
    Base64EncodedText::from(uuid.as_bytes().as_slice())
}

#[cfg(test)]
mod test {
    use crate::crypto::utils::to_id;
    use crate::models::Base64EncodedText;
    use uuid::uuid;

    #[test]
    fn to_id_test() {
        let id = to_id("yay".to_string());
        let expected_uuid = uuid!("f6078ebe-0c2f-08c2-25c0-349aef2fe062");
        let expected_id = Base64EncodedText::from(expected_uuid.as_ref());
        assert_eq!(expected_id, id)
    }
}
