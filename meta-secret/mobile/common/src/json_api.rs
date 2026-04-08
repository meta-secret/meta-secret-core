//! Single JSON-string API for mobile targets (UniFFI, thin JNI/C wrappers).

use meta_secret_core::crypto::key_pair::MasterKeyManager;
use meta_secret_core::crypto::utils::Id48bit;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::ClaimId;
use meta_secret_core::node::common::model::user::common::UserData;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::actions::sign_up::join::JoinActionUpdate;
use serde_json::json;
use std::sync::Arc;

use crate::log_timestamp;
use crate::mobile_manager::MobileApplicationManager;

pub fn generate_master_key() -> String {
    MobileApplicationManager::sync_wrapper(async_generate_master_key())
}

async fn async_generate_master_key() -> String {
    let generated_master_key = MasterKeyManager::generate_sk();
    json!({"success": true, "message": generated_master_key}).to_string()
}

pub fn init_ios(master_key: String) -> String {
    MobileApplicationManager::sync_wrapper(async_init_ios(master_key))
}

async fn async_init_ios(master_key: String) -> String {
    let transport_sk = MasterKeyManager::from_pure_sk(master_key.clone());
    match MobileApplicationManager::init_ios(transport_sk, master_key).await {
        Ok(app_manager) => {
            MobileApplicationManager::set_global_instance(Arc::new(app_manager));
            json!({"success": true, "message": "iOS manager initialized successfully"}).to_string()
        }
        Err(e) => json!({"success": false, "error": format!("{}", e)}).to_string(),
    }
}

pub fn init_android(master_key: String) -> String {
    MobileApplicationManager::sync_wrapper(async_init_android(master_key))
}

async fn async_init_android(master_key: String) -> String {
    let transport_sk = MasterKeyManager::from_pure_sk(master_key.clone());
    match MobileApplicationManager::init_android(transport_sk, master_key).await {
        Ok(app_manager) => {
            MobileApplicationManager::set_global_instance(Arc::new(app_manager));
            json!({"success": true, "message": "Android manager initialized successfully"}).to_string()
        }
        Err(e) => json!({"success": false, "error": format!("{}", e)}).to_string(),
    }
}

pub fn get_state() -> String {
    let _ts = log_timestamp::log_timestamp_utc();
    MobileApplicationManager::sync_wrapper(async_get_state())
}

async fn async_get_state() -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => match app_manager.get_state().await {
            Ok(state) => json!({"success": true, "message": {"state": state}}).to_string(),
            Err(e) => json!({"success": false, "error": format!("App manager is not initialized {e}")}).to_string(),
        },
        None => json!({"success": false, "error": "App manager is not initialized"}).to_string(),
    }
}

pub fn generate_user_creds(vault_name: String) -> String {
    MobileApplicationManager::sync_wrapper(async_generate_user_creds(vault_name))
}

async fn async_generate_user_creds(vault_name: String) -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => match app_manager.generate_user_creds(VaultName::from(vault_name)).await {
            Ok(app_state) => json!({"success": true, "message": {"state": app_state}}).to_string(),
            Err(e) => json!({"success": false, "error": format!("{}", e)}).to_string(),
        },
        None => json!({"success": false, "error": "App manager is not initialized"}).to_string(),
    }
}

pub fn sign_up() -> String {
    MobileApplicationManager::sync_wrapper(async_sign_up())
}

async fn async_sign_up() -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => match app_manager.sign_up().await {
            Ok(state) => json!({"success": true, "message": {"state": state}}).to_string(),
            Err(e) => json!({"success": false, "error": format!("App manager is not initialized: {e}")}).to_string(),
        },
        None => json!({"success": false, "error": "App manager is not initialized"}).to_string(),
    }
}

pub fn update_membership(candidate: String, action_update: String) -> String {
    MobileApplicationManager::sync_wrapper(async_update_membership(candidate, action_update))
}

async fn async_update_membership(candidate: String, action_update: String) -> String {
    let candidate: UserData = match serde_json::from_str(&candidate) {
        Ok(data) => data,
        Err(e) => return json!({"success": false, "error": format!("Failed to parse a candidate: {}", e)}).to_string(),
    };
    let join_action_update: JoinActionUpdate = match serde_json::from_str(&action_update) {
        Ok(data) => data,
        Err(e) => return json!({"success": false, "error": format!("Failed to parse action update: {}", e)}).to_string(),
    };
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => match app_manager.update_membership(candidate, join_action_update).await {
            Ok(_) => json!({"success": true}).to_string(),
            Err(e) => json!({"success": false, "error": format!("Update user join request is failed {e}")}).to_string(),
        },
        None => json!({"success": false, "error": "Update user join request is failed"}).to_string(),
    }
}

pub fn clean_up_database() -> String {
    MobileApplicationManager::sync_wrapper(async_clean_up_database())
}

async fn async_clean_up_database() -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            app_manager.clean_up_database().await;
            json!({"success": true}).to_string()
        }
        None => json!({"success": false, "error": "Cleaning up database failed"}).to_string(),
    }
}

pub fn split_secret(secret_id: String, secret: String) -> String {
    MobileApplicationManager::sync_wrapper(async_split_secret(secret_id, secret))
}

async fn async_split_secret(secret_id: String, secret: String) -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_pass_id = MetaPasswordId::build_from_str(&secret_id);
            let plan_pass_info = PlainPassInfo { pass_id: meta_pass_id, pass: secret };
            app_manager.cluster_distribution(&plan_pass_info).await;
            json!({"success": true}).to_string()
        }
        None => json!({"success": false, "error": "Secret split request is failed"}).to_string(),
    }
}

pub fn find_claim_by(secret_id: String) -> String {
    MobileApplicationManager::sync_wrapper(async_find_claim_by(secret_id))
}

async fn async_find_claim_by(secret_id: String) -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_password_id = MetaPasswordId::build_from_str(&secret_id);
            match app_manager.find_claim_by_pass_id(&meta_password_id).await {
                Some(claim) => json!({"success": true, "message": {"claim": claim}}).to_string(),
                None => json!({"success": false, "error": "Claim has not been found"}).to_string(),
            }
        }
        None => json!({"success": false, "error": "Find claim request is failed"}).to_string(),
    }
}

pub fn find_claim_id_by(secret_id: String) -> String {
    MobileApplicationManager::sync_wrapper(async_find_claim_id_by(secret_id))
}

async fn async_find_claim_id_by(secret_id: String) -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_password_id = MetaPasswordId::build_from_str(&secret_id);
            match app_manager.find_claim_id_by_pass_id(&meta_password_id).await {
                Some(claim) => json!({"success": true, "message": {"claim": claim}}).to_string(),
                None => json!({"success": false, "error": "Claim has not been found"}).to_string(),
            }
        }
        None => json!({"success": false, "error": "Find claim request is failed"}).to_string(),
    }
}

pub fn recover(secret_id: String) -> String {
    MobileApplicationManager::sync_wrapper(async_recover(secret_id))
}

async fn async_recover(secret_id: String) -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_password_id = MetaPasswordId::build_from_str(&secret_id);
            app_manager.recover(&meta_password_id).await;
            json!({"success": true}).to_string()
        }
        None => json!({"success": false, "error": "Recover request is failed"}).to_string(),
    }
}

pub fn accept_recover(claim_id: String) -> String {
    MobileApplicationManager::sync_wrapper(async_accept_recover(claim_id))
}

async fn async_accept_recover(claim_id: String) -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_claim_id = ClaimId::from(Id48bit::from(claim_id));
            match app_manager.accept_recover_mobile(meta_claim_id).await {
                Ok(_) => json!({"success": true}).to_string(),
                Err(e) => json!({"success": false, "error": format!("Accept recover failed: {}", e)}).to_string(),
            }
        }
        None => json!({"success": false, "error": "Accept recover request is failed"}).to_string(),
    }
}

pub fn decline_recover(claim_id: String) -> String {
    MobileApplicationManager::sync_wrapper(async_decline_recover(claim_id))
}

async fn async_decline_recover(claim_id: String) -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_claim_id = ClaimId::from(Id48bit::from(claim_id));
            match app_manager.decline_recover_mobile(meta_claim_id).await {
                Ok(_) => json!({"success": true}).to_string(),
                Err(e) => json!({"success": false, "error": format!("Decline recover failed: {}", e)}).to_string(),
            }
        }
        None => json!({"success": false, "error": "Decline recover request is failed"}).to_string(),
    }
}

pub fn send_decline_completion(claim_id: String) -> String {
    MobileApplicationManager::sync_wrapper(async_send_decline_completion(claim_id))
}

async fn async_send_decline_completion(claim_id: String) -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_claim_id = ClaimId::from(Id48bit::from(claim_id));
            match app_manager.send_decline_completion(meta_claim_id).await {
                Ok(_) => json!({"success": true}).to_string(),
                Err(e) => json!({"success": false, "error": format!("Send decline completion failed: {}", e)}).to_string(),
            }
        }
        None => json!({"success": false, "error": "Send decline completion failed"}).to_string(),
    }
}

pub fn show_recovered(secret_id: String) -> String {
    MobileApplicationManager::sync_wrapper(async_show_recovered(secret_id))
}

async fn async_show_recovered(secret_id: String) -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_password_id = MetaPasswordId::build_from_str(&secret_id);
            match app_manager.show_recovered(&meta_password_id).await {
                Ok(secret) => json!({"success": true, "message": {"secret": secret}}).to_string(),
                Err(e) => json!({"success": false, "error": format!("{}", e)}).to_string(),
            }
        }
        None => json!({"success": false, "error": "Show recovered request is failed"}).to_string(),
    }
}

pub fn meta_ws_start() -> String {
    match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => match app_manager.meta_ws_start_listener() {
            Ok(()) => json!({"success": true}).to_string(),
            Err(e) => json!({"success": false, "error": format!("{}", e)}).to_string(),
        },
        None => json!({"success": false, "error": "App manager is not initialized"}).to_string(),
    }
}

pub fn meta_ws_stop() -> String {
    match crate::meta_ws::meta_ws_stop() {
        Ok(()) => json!({"success": true}).to_string(),
        Err(e) => json!({"success": false, "error": format!("{}", e)}).to_string(),
    }
}

pub fn meta_ws_wait_next_event(timeout_ms: u32) -> bool {
    crate::meta_ws::meta_ws_wait_next_event(timeout_ms)
}
