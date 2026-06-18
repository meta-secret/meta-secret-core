//! UniFFI facade over `mobile_common::json_api` (JSON string payloads).

use meta_secret_core::node::common::model::device::common::{
    device_ui_category, DeviceType, DeviceUiCategory as CoreDeviceUiCategory,
};
use mobile_common::json_api;

uniffi::include_scaffolding!("mobile_uniffi");

fn core_device_ui_to_discriminant(value: CoreDeviceUiCategory) -> i32 {
    match value {
        CoreDeviceUiCategory::Android => 0,
        CoreDeviceUiCategory::Iphone => 1,
        CoreDeviceUiCategory::Tablet => 2,
        CoreDeviceUiCategory::Desktop => 3,
        CoreDeviceUiCategory::Cli => 4,
        CoreDeviceUiCategory::Web => 5,
        CoreDeviceUiCategory::Other => 6,
    }
}

pub fn device_ui_category_discriminant(device_type: String) -> i32 {
    core_device_ui_to_discriminant(device_ui_category(&DeviceType::from(device_type)))
}

#[cfg(target_os = "android")]
use jni::objects::{JClass, JObject};
#[cfg(target_os = "android")]
use jni::sys::jboolean;
#[cfg(target_os = "android")]
use jni::JNIEnv;

pub fn generate_master_key() -> String {
    json_api::generate_master_key()
}

pub fn init_ios(master_key: String) -> String {
    json_api::init_ios(master_key)
}

pub fn init_android(master_key: String) -> String {
    json_api::init_android(master_key)
}

pub fn init_ios_with_device(master_key: String, device_name: String, device_type: String) -> String {
    json_api::init_ios_with_device(master_key, device_name, device_type)
}

pub fn init_android_with_device(master_key: String, device_name: String, device_type: String) -> String {
    json_api::init_android_with_device(master_key, device_name, device_type)
}

pub fn get_state() -> String {
    json_api::get_state()
}

pub fn generate_user_creds(vault_name: String) -> String {
    json_api::generate_user_creds(vault_name)
}

pub fn sign_up() -> String {
    json_api::sign_up()
}

pub fn update_membership(candidate: String, action_update: String) -> String {
    json_api::update_membership(candidate, action_update)
}

pub fn clean_up_database() -> String {
    json_api::clean_up_database()
}

pub fn split_secret(secret_id: String, secret: String) -> String {
    json_api::split_secret(secret_id, secret)
}

pub fn find_claim_by(secret_id: String) -> String {
    json_api::find_claim_by(secret_id)
}

pub fn find_claim_id_by(secret_id: String) -> String {
    json_api::find_claim_id_by(secret_id)
}

pub fn recover(secret_id: String) -> String {
    json_api::recover(secret_id)
}

pub fn accept_recover(claim_id: String) -> String {
    json_api::accept_recover(claim_id)
}

pub fn decline_recover(claim_id: String) -> String {
    json_api::decline_recover(claim_id)
}

pub fn send_decline_completion(claim_id: String) -> String {
    json_api::send_decline_completion(claim_id)
}

pub fn show_recovered(secret_id: String) -> String {
    json_api::show_recovered(secret_id)
}

#[cfg(test)]
mod device_ui_category_ffi_tests {
    use super::device_ui_category_discriminant;

    #[test]
    fn discriminant_matches_wasm_ts_order() {
        assert_eq!(device_ui_category_discriminant("Android".to_string()), 0);
        assert_eq!(device_ui_category_discriminant("Web".to_string()), 5);
        assert_eq!(device_ui_category_discriminant("my android phone".to_string()), 0);
        assert_eq!(device_ui_category_discriminant("unknown-thing".to_string()), 6);
    }
}

/// Android-only bootstrap for rustls platform verifier.
///
/// Must be called from app startup before any network-backed state fetch.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_metasecret_core_MetaSecretNative_initRustlsPlatformVerifier(
    mut env: JNIEnv,
    _class: JClass,
    context: JObject,
) -> jboolean {
    match rustls_platform_verifier::android::init_with_env(&mut env, context) {
        Ok(()) => 1,
        Err(err) => {
            eprintln!("Failed to initialize rustls-platform-verifier: {err}");
            0
        }
    }
}
