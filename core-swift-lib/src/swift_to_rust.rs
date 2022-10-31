use crate::strings::RustByteSlice;
use meta_secret_core::crypto::encoding::serialized_key_manager::SerializedKeyManager;
use meta_secret_core::crypto::encoding::Base64EncodedText;
use meta_secret_core::crypto::keys::{AeadCipherText, KeyManager};
use meta_secret_core::shared_secret;
use meta_secret_core::shared_secret::data_block::common::SharedSecretConfig;
use meta_secret_core::shared_secret::shared_secret::UserShareDto;
use serde::{Deserialize, Serialize};
use std::slice;
use std::str;

type SizeT = usize;

//STRUCTURES
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonMappedData {
    vault_name: String,
    sender_key_manager: SerializedKeyManager,
    receivers_pub_keys: Vec<Base64EncodedText>,
    secret: String,
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserSignature {
    vault_name: String,
    signature: String,
    key_manager: SerializedKeyManager,
}

//LIB METHODS
#[no_mangle]
pub extern "C" fn generate(json_bytes: *const u8, json_len: SizeT) -> RustByteSlice {
    let json_string = data_to_json_string(json_bytes, json_len);
    let mut user_signature: UserSignature = serde_json::from_str(&*json_string).unwrap();

    let name = user_signature.vault_name.clone();

    let serialized_key_manager = new_keys_pair_internal();
    let key_manager = KeyManager::from(&serialized_key_manager);
    let signature = key_manager.dsa.sign(name);

    user_signature.signature = signature.base64_text;
    user_signature.key_manager = serialized_key_manager;

    let user = serde_json::to_string_pretty(&user_signature).unwrap();
    RustByteSlice {
        bytes: user.as_ptr(),
        len: user.len() as SizeT,
    }
}

#[no_mangle]
pub extern "C" fn split_and_encode(json_bytes: *const u8, json_len: SizeT) -> RustByteSlice {
    // Constants & Properties
    let cfg = SharedSecretConfig {
        number_of_shares: 3,
        threshold: 2,
    };

    // JSON parsing
    let json_string: String = data_to_json_string(json_bytes, json_len);
    let json_struct: JsonMappedData = serde_json::from_str(&*json_string).unwrap();

    let encrypted_shares_json = split_and_encode_internal(cfg, &json_struct);
    RustByteSlice {
        bytes: encrypted_shares_json.as_ptr(),
        len: encrypted_shares_json.len() as SizeT,
    }
}

fn split_and_encode_internal(cfg: SharedSecretConfig, json_struct: &JsonMappedData) -> String {
    let sender_key_manager = KeyManager::from(&json_struct.sender_key_manager);

    // Split
    let top_secret_password = json_struct.secret.clone();
    let shares: Vec<UserShareDto> = shared_secret::split(top_secret_password.clone(), cfg);

    // Create KeyManager

    // Encrypt shares
    let mut encrypted_shares: Vec<AeadCipherText> = Vec::new();
    for i in 0..shares.len() {
        let password_share: &UserShareDto = &shares[i];
        let password_share: String = serde_json::to_string(&password_share).unwrap();

        let receiver_pk = json_struct.receivers_pub_keys[i].clone();

        let encrypted_share: AeadCipherText = sender_key_manager
            .transport_key_pair
            .encrypt_string(password_share, receiver_pk);

        encrypted_shares.push(encrypted_share);
    }

    // Shares to JSon
    let encrypted_shares_json = serde_json::to_string_pretty(&encrypted_shares).unwrap();
    encrypted_shares_json
}

//PRIVATE METHODS
fn data_to_json_string(json_bytes: *const u8, json_len: SizeT) -> String {
    // JSON parsing
    let json_bytes_slice = unsafe { slice::from_raw_parts(json_bytes, json_len as usize) };
    let json_string = str::from_utf8(json_bytes_slice).unwrap();
    json_string.to_string()
}

pub fn new_keys_pair_internal() -> SerializedKeyManager {
    let key_manager = KeyManager::generate();
    SerializedKeyManager::from(&key_manager)
}

//TESTS
#[cfg(test)]
pub mod test {
    use crate::rust_to_swift::new_keys_pair_internal;
    use crate::swift_to_rust::{split_and_encode_internal, JsonMappedData};
    use meta_secret_core::crypto::key_pair::KeyPair;
    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::shared_secret::data_block::common::SharedSecretConfig;

    #[test]
    fn split_and_encode() {
        let keys_pair = new_keys_pair_internal();

        // Constants & Properties
        let cfg = SharedSecretConfig {
            number_of_shares: 3,
            threshold: 2,
        };

        let km_2 = KeyManager::generate();
        let km_3 = KeyManager::generate();

        let data = JsonMappedData {
            vault_name: "test_vault".to_string(),
            sender_key_manager: keys_pair.clone(),
            receivers_pub_keys: vec![
                keys_pair.transport.public_key,
                km_2.transport_key_pair.public_key(),
                km_3.transport_key_pair.public_key(),
            ],
            secret: "top_secret".to_string(),
        };

        let json_result = split_and_encode_internal(cfg, &data);
        println!("{}", json_result);
    }
}
