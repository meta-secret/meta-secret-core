//! UniFFI facade over `mobile_common::json_api` (JSON string payloads).

use mobile_common::json_api;

uniffi::include_scaffolding!("mobile_uniffi");

pub fn generate_master_key() -> String {
    json_api::generate_master_key()
}

pub fn init_ios(master_key: String) -> String {
    json_api::init_ios(master_key)
}

pub fn init_android(master_key: String) -> String {
    json_api::init_android(master_key)
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

pub fn meta_ws_start() -> String {
    json_api::meta_ws_start()
}

pub fn meta_ws_stop() -> String {
    json_api::meta_ws_stop()
}

pub fn meta_ws_wait_next_event(timeout_ms: u32) -> bool {
    json_api::meta_ws_wait_next_event(timeout_ms)
}
