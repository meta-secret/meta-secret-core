use meta_secret_core::crypto::encoding::base64::Base64EncodedText;
use meta_secret_core::crypto::encoding::serialized_key_manager::SerializedKeyManager;
use meta_secret_core::crypto::key_pair::DecryptionDirection;
use meta_secret_core::crypto::keys::{AeadCipherText, AeadPlainText, KeyManager};
use meta_secret_core::errors::CoreError;
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

impl TryFrom<&String> for JsonMappedData {
    type Error = CoreError;

    fn try_from(data_string: &String) -> Result<Self, Self::Error> {
        let json = serde_json::from_str(data_string)?;
        Ok(json)
    }
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
pub extern "C" fn generate_signed_user(vault_name_bytes: *const u8, len: SizeT) -> *mut c_char {
    let user = generate_signed_user_private(vault_name_bytes, len).unwrap();
    CString::new(user).unwrap().into_raw()
}

fn generate_signed_user_private(vault_name_bytes: *const u8, len: SizeT) -> Result<String, CoreError> {
    let device_name = data_to_string(vault_name_bytes, len)?;

    let key_manager = KeyManager::generate();

    let signature = key_manager.dsa.sign(device_name);
    let security_box = UserSecurityBox {
        signature,
        key_manager: SerializedKeyManager::from(&key_manager),
    };

    let user = serde_json::to_string_pretty(&security_box)?;
    Ok(user)
}

// Split
#[no_mangle]
pub extern "C" fn split_secret(strings_bytes: *const u8, string_len: SizeT) -> *mut c_char {
    let result_json = split_secret_internal(strings_bytes, string_len).unwrap();
    CString::new(result_json).unwrap().into_raw()
}

fn split_secret_internal(strings_bytes: *const u8, string_len: SizeT) -> Result<String, CoreError> {
    // Constants & Properties
    let cfg = SharedSecretConfig {
        number_of_shares: 3,
        threshold: 2,
    };

    // JSON parsing
    let json_string: String = data_to_string(strings_bytes, string_len)?;
    let shares: Vec<UserShareDto> = shared_secret::split(json_string, cfg);

    // Shares to JSon
    let result_json = serde_json::to_string_pretty(&shares)?;
    Ok(result_json)
}

#[no_mangle]
pub extern "C" fn generate_meta_password_id(password_id: *const u8, json_len: SizeT) -> *mut c_char {
    let result_json = generate_meta_password_id_internal(password_id, json_len).unwrap();
    CString::new(result_json).unwrap().into_raw()
}

fn generate_meta_password_id_internal(password_id: *const u8, json_len: SizeT) -> Result<String, CoreError> {
    let password_id = data_to_string(password_id, json_len)?;
    let meta_password_id = MetaPasswordId::generate(password_id);

    // Shares to JSon
    let result_json = serde_json::to_string_pretty(&meta_password_id)?;
    Ok(result_json)
}

#[no_mangle]
pub extern "C" fn encrypt_secret(json_bytes: *const u8, json_len: SizeT) -> *mut c_char {
    let encrypted_shares_json = encrypt_secret_internal(json_bytes, json_len).unwrap();
    CString::new(encrypted_shares_json).unwrap().into_raw()
}

fn encrypt_secret_internal(bytes: *const u8, len: SizeT) -> Result<String, CoreError> {
    let data_string: String = data_to_string(bytes, len)?;

    let json_struct = JsonMappedData::try_from(&data_string)?;
    let key_manager = KeyManager::try_from(&json_struct.sender_key_manager)?;

    // Encrypt shares
    let encrypted_share: AeadCipherText = key_manager
        .transport_key_pair
        .encrypt_string(json_struct.secret, json_struct.receiver_pub_key)?;

    // Shares to JSon
    let encrypted_shares_json = serde_json::to_string(&encrypted_share)?;
    Ok(encrypted_shares_json)
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
struct RestoreTask {
    key_manager: SerializedKeyManager,
    doc_one: SecretDistributionDocData,
    doc_two: SecretDistributionDocData,
}

impl TryFrom<&String> for RestoreTask {
    type Error = CoreError;

    fn try_from(data: &String) -> Result<Self, Self::Error> {
        let json = serde_json::from_str(data)?;
        Ok(json)
    }
}

#[no_mangle]
pub extern "C" fn restore_secret(bytes: *const u8, len: SizeT) -> *mut c_char {
    let recovered_secret = recover_secret_internal(bytes, len).unwrap();
    CString::new(recovered_secret).unwrap().into_raw()
}

fn recover_secret_internal(bytes: *const u8, len: SizeT) -> Result<String, CoreError> {
    let data_string: String = data_to_string(bytes, len)?;
    let restore_task = RestoreTask::try_from(&data_string)?;

    let key_manager = KeyManager::try_from(&restore_task.key_manager)?;
    let share_from_device_2_json: AeadPlainText = key_manager.transport_key_pair.decrypt(
        &restore_task.doc_two.secret_message.encrypted_text,
        DecryptionDirection::Backward,
    )?;
    let share_from_device_2_json = UserShareDto::try_from(&share_from_device_2_json.msg)?;

    let share_from_device_1_json: AeadPlainText = key_manager.transport_key_pair.decrypt(
        &restore_task.doc_one.secret_message.encrypted_text,
        DecryptionDirection::Backward,
    )?;

    let share_from_device_1_json = UserShareDto::try_from(&share_from_device_1_json.msg)?;

    // Restored Password to JSon
    let password = recover_from_shares(vec![share_from_device_2_json, share_from_device_1_json])?;
    let result_json = serde_json::to_string_pretty(&password)?;
    Ok(result_json)
}

//PRIVATE METHODS
fn data_to_string(bytes: *const u8, len: SizeT) -> Result<String, CoreError> {
    // JSON parsing
    let bytes_slice = unsafe { slice::from_raw_parts(bytes, len as usize) };
    let result = str::from_utf8(bytes_slice)?;
    Ok(result.to_string())
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
