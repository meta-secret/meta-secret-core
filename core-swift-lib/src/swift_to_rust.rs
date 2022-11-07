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
    sender_key_manager: SerializedKeyManager,
    receivers_pub_keys: String,
    secret: String,
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserSignature {
    vault_name: String,
    signature: String,
    device_id: String,
    device_name: String,
    key_manager: Option<SerializedKeyManager>,
}

//LIB METHODS

//Generate vault and sign
#[no_mangle]
pub extern "C" fn generate_signed_user(json_bytes: *const u8, json_len: SizeT) -> RustByteSlice {
    let json_string = data_to_json_string(json_bytes, json_len);
    let mut user_signature: UserSignature = serde_json::from_str(&*json_string).unwrap();

    let name = user_signature.vault_name.clone();

    let serialized_key_manager = new_keys_pair_internal();
    let key_manager = KeyManager::from(&serialized_key_manager);
    let signature = key_manager.dsa.sign(name);

    user_signature.signature = signature.base64_text;
    user_signature.key_manager = Option::from(serialized_key_manager);

    let user = serde_json::to_string_pretty(&user_signature).unwrap();
    RustByteSlice {
        bytes: user.as_ptr(),
        len: user.len() as SizeT,
    }
}

// Split
#[no_mangle]
pub extern "C" fn split_secret(strings_bytes: *const u8, string_len: SizeT) -> RustByteSlice {
    // Constants & Properties
    let cfg = SharedSecretConfig {
        number_of_shares: 3,
        threshold: 2,
    };

    // JSON parsing
    let json_string: String = data_to_json_string(strings_bytes, string_len);
    let shares: Vec<UserShareDto> = shared_secret::split(json_string.clone(), cfg);

    // Shares to JSon
    let result_json = serde_json::to_string_pretty(&shares).unwrap();

    RustByteSlice {
        bytes: result_json.as_ptr(),
        len: result_json.len() as SizeT,
    }
}

#[no_mangle]
pub extern "C" fn encode_secret(json_bytes: *const u8, json_len: SizeT) -> RustByteSlice {
    // JSON parsing
    let json_string: String = data_to_json_string(json_bytes, json_len);
    let json_struct: JsonMappedData = serde_json::from_str(&*json_string).unwrap();

    let sender_key_manager = KeyManager::from(&json_struct.sender_key_manager);

    // Encrypt shares
    let password_share: String = json_struct.secret;
    let receiver_pk = json_struct.receivers_pub_keys.clone();

    let encrypted_share: AeadCipherText = sender_key_manager
        .transport_key_pair
        .encrypt_string(password_share, Base64EncodedText::from(receiver_pk.as_bytes()));


    // Shares to JSon
    let encrypted_shares_json = serde_json::to_string_pretty(&encrypted_share).unwrap();

    RustByteSlice {
        bytes: encrypted_shares_json.as_ptr(),
        len: encrypted_shares_json.len() as SizeT,
    }
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