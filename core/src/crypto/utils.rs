use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha256};

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
