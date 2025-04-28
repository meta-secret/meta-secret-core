use anyhow::Context;
use anyhow::Result;
use meta_secret_core::CoreResult;
use meta_secret_core::crypto::keys::{KeyManager, SecretBox, TransportPk};
use meta_secret_core::errors::CoreError;
use meta_secret_core::node::common::model::secret::SecretDistributionData;
use meta_secret_core::recover_from_shares;
use meta_secret_core::secret::data_block::common::SharedSecretConfig;
use meta_secret_core::secret::shared_secret::UserShareDto;
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::os::raw::c_char;
use std::slice;
use std::str;

type SizeT = usize;

//LIB METHODS

//Generate vault and sign
#[unsafe(no_mangle)]
pub extern "C" fn generate_signed_user(vault_name_bytes: *const u8, len: SizeT) -> *mut c_char {
    let user = internal::generate_security_box(vault_name_bytes, len)
        .with_context(|| "Error: Signature generation failed".to_string())
        .unwrap();
    to_c_str(user)
}

// Split
#[unsafe(no_mangle)]
pub extern "C" fn split_secret(strings_bytes: *const u8, string_len: SizeT) -> *mut c_char {
    let result_json = internal::split_secret(strings_bytes, string_len)
        .with_context(|| "Error: secret splitting operation failed".to_string())
        .unwrap();
    to_c_str(result_json)
}

#[unsafe(no_mangle)]
pub extern "C" fn generate_meta_password_id(password_id: *const u8, json_len: SizeT) -> *mut c_char {
    let result_json = internal::generate_meta_password_id(password_id, json_len)
        .with_context(|| "Error: meta password id generation failed".to_string())
        .unwrap();
    to_c_str(result_json)
}

#[unsafe(no_mangle)]
pub extern "C" fn encrypt_secret(json_bytes: *const u8, json_len: SizeT) -> *mut c_char {
    let encrypted_shares_json = internal::encrypt_secret(json_bytes, json_len)
        .with_context(|| "Error: encryption operation failed".to_string())
        .unwrap();
    to_c_str(encrypted_shares_json)
}

#[unsafe(no_mangle)]
pub extern "C" fn decrypt_secret(json_bytes: *const u8, json_len: SizeT) -> *mut c_char {
    let decrypted_shares_json = internal::decrypt_secret(json_bytes, json_len).unwrap();
    to_c_str(decrypted_shares_json)
}

#[unsafe(no_mangle)]
pub extern "C" fn restore_secret(bytes: *const u8, len: SizeT) -> *mut c_char {
    let recovered_secret = internal::recover_secret(bytes, len)
        .with_context(|| "Secret recovery error".to_string())
        .unwrap();

    to_c_str(recovered_secret)
}

fn to_c_str(str: String) -> *mut c_char {
    CString::new(str)
        .with_context(|| "String transformation error".to_string())
        .unwrap()
        .into_raw()
}

mod internal {
    use super::*;
    use meta_secret_core::node::common::model::crypto::aead::{AeadCipherText, AeadPlainText, EncryptedMessage};
    use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo};
    use meta_secret_core::secret;
    use meta_secret_core::secret::shared_secret::PlainText;

    #[allow(unused_variables)]
    pub fn generate_security_box(vault_name_bytes: *const u8, len: SizeT) -> CoreResult<String> {
        let security_box = KeyManager::generate_secret_box();
        let user = serde_json::to_string_pretty(&security_box)?;
        Ok(user)
    }

    pub fn split_secret(strings_bytes: *const u8, string_len: SizeT) -> CoreResult<String> {
        // Constants & Properties
        let cfg = SharedSecretConfig {
            number_of_shares: 3,
            threshold: 2,
        };

        // JSON parsing
        let pass = data_to_string(strings_bytes, string_len)?;
        let plain_pass = PlainPassInfo::new(String::from("test"), pass);
        let pass_info = SecurePassInfo::from(plain_pass);
        let shares: Vec<UserShareDto> = secret::split(pass_info, cfg)?;

        // Shares to JSon
        let result_json = serde_json::to_string_pretty(&shares)?;
        Ok(result_json)
    }

    pub fn generate_meta_password_id(password_id: *const u8, json_len: SizeT) -> CoreResult<String> {
        let password_id = data_to_string(password_id, json_len)?;
        let meta_password_id = MetaPasswordId::build(password_id);

        // Shares to JSon
        let result_json = serde_json::to_string_pretty(&meta_password_id)?;
        Ok(result_json)
    }

    pub fn encrypt_secret(bytes: *const u8, len: SizeT) -> Result<String> {
        let data_string: String = data_to_string(bytes, len)?;

        let json_struct = JsonMappedData::try_from(&data_string)?;
        let key_manager = KeyManager::try_from(&json_struct.sender_key_manager)?;

        // Encrypt shares
        let encrypted_share: AeadCipherText = key_manager
            .transport
            .encrypt_string(PlainText::from(json_struct.secret), &json_struct.receiver_pub_key)?;

        // Shares to JSon
        let encrypted_shares_json = serde_json::to_string(&encrypted_share)?;
        Ok(encrypted_shares_json)
    }

    pub fn decrypt_secret(bytes: *const u8, len: SizeT) -> Result<String> {
        let data_string: String = data_to_string(bytes, len)?;
        let restore_task = DecryptTask::try_from(&data_string)?;
        let key_manager = KeyManager::try_from(&restore_task.key_manager)?;

        println!("restore_task {:?}", restore_task.doc);
        // Decrypt shares
        let EncryptedMessage::CipherShare { share, .. } = restore_task.doc.secret_message;
        let share_json: AeadPlainText = share.decrypt(&key_manager.transport.sk())?;
        let share_json = UserShareDto::try_from(&share_json.msg)?;

        // Decrypted Share to JSon
        let result_json = serde_json::to_string_pretty(&share_json)?;
        Ok(result_json)
    }

    pub fn recover_secret(bytes: *const u8, len: SizeT) -> Result<String> {
        let data_string: String = data_to_string(bytes, len)?;
        let restore_task = RestoreTask::try_from(&data_string)?;

        let key_manager = KeyManager::try_from(&restore_task.key_manager)?;
        let EncryptedMessage::CipherShare {
            share: second_share, ..
        } = restore_task.doc_two.secret_message;
        let share_from_device_2_json: AeadPlainText = second_share.decrypt(&key_manager.transport.sk())?;
        let share_from_device_2_json = UserShareDto::try_from(&share_from_device_2_json.msg)?;

        let EncryptedMessage::CipherShare { share: first_share, .. } = restore_task.doc_one.secret_message;
        let share_from_device_1_json: AeadPlainText = first_share.decrypt(&key_manager.transport.sk())?;

        let share_from_device_1_json = UserShareDto::try_from(&share_from_device_1_json.msg)?;

        // Restored Password to JSon
        let password = recover_from_shares(vec![share_from_device_2_json, share_from_device_1_json])?;
        let result_json = serde_json::to_string_pretty(&password)?;
        Ok(result_json)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_string_free(s: *mut c_char) {
    unsafe {
        if s.is_null() {
            return;
        }
        let _ = CString::from_raw(s);
    };
}

fn data_to_string(bytes: *const u8, len: SizeT) -> CoreResult<String> {
    // JSON parsing
    let bytes_slice = unsafe { slice::from_raw_parts(bytes, len) };
    let result = str::from_utf8(bytes_slice)?;
    Ok(result.to_string())
}

//STRUCTURES
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonMappedData {
    sender_key_manager: SecretBox,
    receiver_pub_key: TransportPk,
    secret: String,
}

impl TryFrom<&String> for JsonMappedData {
    type Error = CoreError;

    fn try_from(data_string: &String) -> Result<Self, Self::Error> {
        let json = serde_json::from_str(data_string)?;
        Ok(json)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreTask {
    key_manager: SecretBox,
    doc_one: SecretDistributionData,
    doc_two: SecretDistributionData,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DecryptTask {
    key_manager: SecretBox,
    doc: SecretDistributionData,
}

impl TryFrom<&String> for RestoreTask {
    type Error = CoreError;

    fn try_from(data: &String) -> Result<Self, Self::Error> {
        let json = serde_json::from_str(data)?;
        Ok(json)
    }
}

impl TryFrom<&String> for DecryptTask {
    type Error = CoreError;

    fn try_from(data: &String) -> Result<Self, Self::Error> {
        let json = serde_json::from_str(data)?;
        Ok(json)
    }
}

//TESTS
#[cfg(test)]
pub mod test {
    use meta_secret_core::crypto::key_pair::KeyPair;
    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::node::common::model::crypto::aead::AeadCipherText;
    use meta_secret_core::node::common::model::meta_pass::{PlainPassInfo, SecurePassInfo};
    use meta_secret_core::secret;
    use meta_secret_core::secret::data_block::common::SharedSecretConfig;
    use meta_secret_core::secret::shared_secret::{PlainText, UserShareDto};

    #[test]
    fn split_and_encrypt() -> anyhow::Result<()> {
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
        let shares: Vec<UserShareDto> = {
            let plain_pass = PlainPassInfo::new("test".to_string(), "Secret".to_string());
            secret::split(SecurePassInfo::from(plain_pass), cfg)?
        };

        // Encrypt shares
        let secret = shares[0].clone();
        let password_share_text_base64 = secret.share_blocks[0].data.clone().base64_str();
        let receiver_pk = key_manager_2.transport.pk();
        let encrypted_share: AeadCipherText = key_manager_1
            .transport
            .encrypt_string(PlainText::from(password_share_text_base64), &receiver_pk)?;

        println!("result {:?}", encrypted_share);

        Ok(())
    }
}
