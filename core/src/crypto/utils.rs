use base64::alphabet::URL_SAFE;
use base64::engine::fast_portable::{FastPortable, NO_PAD};
use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const URL_SAFE_ENGINE: FastPortable = FastPortable::from(&URL_SAFE, NO_PAD);

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
pub fn rand_uuid_b64_url_enc() -> String {
    let uuid = Uuid::new_v4();
    let uuid_bytes = uuid.as_bytes();

    base64::encode_engine(uuid_bytes, &URL_SAFE_ENGINE)
}
