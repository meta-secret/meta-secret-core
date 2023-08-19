use image::EncodableLayout;
use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::Base64EncodedText;

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
    let uuid_bytes = uuid.as_bytes().as_slice();
    Base64EncodedText::from(uuid_bytes)
}

pub fn rand_64bit_b64_url_enc() -> Base64EncodedText {
    let uuid = Uuid::new_v4().as_u64_pair().0.to_le_bytes().to_vec();
    Base64EncodedText::from(uuid)
}

pub fn generate_uuid_b64_url_enc(value: String) -> String {
    let hash = Sha256::digest(value.as_bytes());
    let uuid = Uuid::from_slice(&hash.as_slice()[..16]).unwrap();
    Base64EncodedText::from(uuid.as_bytes().as_slice()).base64_text
}

/// Convert a string to a base64 url encoded uuid
pub fn to_id(str: &str) -> String {
    //let hash = Sha256::digest(str.as_bytes());
    //let uuid = Uuid::from_slice(&hash.as_slice()[..16]).unwrap();
    //Base64EncodedText::from(uuid.as_bytes().as_slice())
    //Base64EncodedText::from(uuid.as_bytes().as_slice())
    let next = if str.contains("::") {
        let parts: Vec<&str> = str.split("::").collect();
        let next_counter: usize = parts[1].parse().unwrap();
        format!("{}::{:?}", parts[0], next_counter + 1)
    } else {
        format!("{}::{}", str, 0)
    };

    //Base64EncodedText::from(next)
    next
}

#[cfg(test)]
mod test {
    use crate::crypto::utils::to_id;

    #[test]
    fn to_id_test() {
        //let id = to_id("yay");
        //let expected_uuid = uuid!("f6078ebe-0c2f-08c2-25c0-349aef2fe062").as_ref().as_bytes();
        //let expected_uuid = String::from_utf8(expected_uuid.to_vec()).unwrap();
        //let expected_uuid = Base64EncodedText::from(expected_uuid.as_ref());
        //assert_eq!(expected_uuid, id)

        let id_0 = to_id("qwe:qwe");
        println!("{}", id_0);
        let id_0 = to_id(id_0.as_str());
        println!("{}", id_0);
        let id_0 = to_id(id_0.as_str());
        println!("{}", id_0);
        let id_0 = to_id(id_0.as_str());
        println!("{}", id_0);
        let id_0 = to_id(id_0.as_str());
        println!("{}", id_0);
        let id_0 = to_id(id_0.as_str());
        println!("{}", id_0);
    }
}
