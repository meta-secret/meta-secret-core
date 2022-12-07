use meta_secret_core::crypto::encoding::base64::Base64EncodedText;
use meta_secret_core::crypto::encoding::serialized_key_manager::SerializedKeyManager;
use meta_secret_core::crypto::key_pair::DecryptionDirection;
use meta_secret_core::crypto::keys::{AeadCipherText, AeadPlainText, KeyManager};
use meta_secret_core::sdk::api::SecretDistributionDocData;
use meta_secret_core::sdk::password::MetaPasswordId;
use meta_secret_core::shared_secret::data_block::common::SharedSecretConfig;
use meta_secret_core::shared_secret::shared_secret::UserShareDto;
use meta_secret_core::{recover_from_shares, shared_secret};
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::os::raw::c_char;
use std::slice;
use std::str;

type SizeT = usize;

//STRUCTURES
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonMappedData {
    sender_key_manager: SerializedKeyManager,
    receiver_pub_key: Base64EncodedText,
    secret: String,
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserSecurityBox {
    signature: Base64EncodedText,
    key_manager: SerializedKeyManager,
}

//LIB METHODS

//Generate vault and sign
#[no_mangle]
pub extern "C" fn generate_signed_user(vault_name_bytes: *const u8, json_len: SizeT) -> *mut c_char {
    let device_name = data_to_json_string(vault_name_bytes, json_len);

    let key_manager = KeyManager::generate();

    let signature = key_manager.dsa.sign(device_name);
    let security_box = UserSecurityBox {
        signature,
        key_manager: SerializedKeyManager::from(&key_manager),
    };

    let user = serde_json::to_string_pretty(&security_box).unwrap();
    CString::new(user).unwrap().into_raw()
}

// Split
#[no_mangle]
pub extern "C" fn split_secret(strings_bytes: *const u8, string_len: SizeT) -> *mut c_char {
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
    CString::new(result_json).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn generate_meta_password_id(password_id: *const u8, json_len: SizeT) -> *mut c_char {
    let password_id = data_to_json_string(password_id, json_len);
    let meta_password_id = MetaPasswordId::generate(password_id);

    // Shares to JSon
    let result_json = serde_json::to_string_pretty(&meta_password_id).unwrap();
    CString::new(result_json).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn encrypt_secret(json_bytes: *const u8, json_len: SizeT) -> *mut c_char {
    // JSON parsing
    let json_string: String = data_to_json_string(json_bytes, json_len);

    let json_struct: JsonMappedData = serde_json::from_str(&json_string).unwrap();
    let key_manager = KeyManager::try_from(&json_struct.sender_key_manager).unwrap();

    // Encrypt shares
    let encrypted_share: AeadCipherText = key_manager
        .transport_key_pair
        .encrypt_string(json_struct.secret, json_struct.receiver_pub_key)
        .unwrap();

    // Shares to JSon
    let encrypted_shares_json = serde_json::to_string(&encrypted_share).unwrap();
    CString::new(encrypted_shares_json).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn rust_string_free(s: *mut c_char) {
    unsafe {
        if s.is_null() {
            return;
        }
        CString::from_raw(s)
    };
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreRequest {
    key_manager: SerializedKeyManager,
    doc_one: SecretDistributionDocData,
    doc_two: SecretDistributionDocData,
}

#[no_mangle]
pub extern "C" fn restore_secret(json_bytes: *const u8, json_len: SizeT) -> *mut c_char {
    // JSON parsing
    let json_string: String = data_to_json_string(json_bytes, json_len);
    let json_struct: RestoreRequest = serde_json::from_str(&json_string).unwrap();

    let key_manager = KeyManager::try_from(&json_struct.key_manager).unwrap();
    let share_from_device_2_json: AeadPlainText = key_manager
        .transport_key_pair
        .decrypt(
            &json_struct.doc_two.secret_message.encrypted_text,
            DecryptionDirection::Backward,
        )
        .unwrap();
    let share_from_device_2_json: UserShareDto = serde_json::from_str(&share_from_device_2_json.msg).unwrap();

    let share_from_device_1_json: AeadPlainText = key_manager
        .transport_key_pair
        .decrypt(
            &json_struct.doc_one.secret_message.encrypted_text,
            DecryptionDirection::Backward,
        )
        .unwrap();
    let share_from_device_1_json: UserShareDto = serde_json::from_str(&share_from_device_1_json.msg).unwrap();

    // Restored Password to JSon
    let password = recover_from_shares(vec![share_from_device_2_json, share_from_device_1_json]).unwrap();
    let result_json = serde_json::to_string_pretty(&password).unwrap();

    CString::new(result_json).unwrap().into_raw()
}

//PRIVATE METHODS
fn data_to_json_string(json_bytes: *const u8, json_len: SizeT) -> String {
    // JSON parsing
    let json_bytes_slice = unsafe { slice::from_raw_parts(json_bytes, json_len as usize) };
    let json_string = str::from_utf8(json_bytes_slice).unwrap();
    json_string.to_string()
}

//TESTS
#[cfg(test)]
pub mod test {
    use meta_secret_core::crypto::key_pair::KeyPair;
    use meta_secret_core::crypto::keys::{AeadCipherText, KeyManager};
    use meta_secret_core::shared_secret;
    use meta_secret_core::shared_secret::data_block::common::SharedSecretConfig;
    use meta_secret_core::shared_secret::shared_secret::UserShareDto;

    #[test]
    fn split_and_encrypt() {
        // Constants & Properties
        let cfg = SharedSecretConfig {
            number_of_shares: 3,
            threshold: 2,
        };

        // User one
        let key_manager_1 = KeyManager::generate();

        // User two
        let key_manager_2 = KeyManager::generate();

        // Split
        let shares: Vec<UserShareDto> = shared_secret::split("Secret".to_string(), cfg);

        // Encrypt shares
        let secret = shares[0].clone();
        let password_share: String = secret.share_blocks[0].data.base64_text.clone();
        let receiver_pk = key_manager_2.transport_key_pair.public_key();
        let encrypted_share: AeadCipherText = key_manager_1
            .transport_key_pair
            .encrypt_string(password_share, receiver_pk)
            .unwrap();

        println!("result {:?}", encrypted_share);
    }
}
