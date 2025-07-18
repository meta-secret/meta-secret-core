use std::ffi::{CString};
use std::os::raw::c_char;
use meta_secret_core::crypto::key_pair::MasterKeyManager;
use serde_json::json;
use mobile_common::mobile_manager::MobileApplicationManager;
use std::sync::Arc;
use meta_secret_core::node::common::model::user::common::UserData;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::actions::sign_up::join::JoinActionUpdate;

fn json_to_c_string(json: &str) -> *mut c_char {
    CString::new(json).unwrap().into_raw()
}

fn c_str_to_string(ptr: *const c_char) -> String {
    if !ptr.is_null() {
        unsafe {
            std::ffi::CStr::from_ptr(ptr)
                .to_string_lossy()
                .to_string()
        }
    } else {
        String::from("")
    }
}

// MARK: Generating / init
#[unsafe(no_mangle)]
pub extern "C" fn generate_master_key() -> *mut c_char {
    MobileApplicationManager::sync_wrapper(async_generate_master_key())
}

async fn async_generate_master_key() -> *mut c_char {
    let generated_master_key = MasterKeyManager::generate_sk();
    let response = json!({
        "success": true,
        "message": generated_master_key
    });
    json_to_c_string(&response.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn init(master_key_ptr: *const c_char) -> *mut c_char {
    let master_key = c_str_to_string(master_key_ptr);
    MobileApplicationManager::sync_wrapper(async_init(master_key))
}

async fn async_init(master_key: String) -> *mut c_char {
    let transport_sk = MasterKeyManager::from_pure_sk(master_key);
    
    let result = match MobileApplicationManager::init_ios(transport_sk).await {
        Ok(app_manager) => {
            MobileApplicationManager::set_global_instance(Arc::new(app_manager));
            json!({
                    "success": true,
                    "message": "iOS manager initialized successfully"
                }).to_string()
        },
        Err(e) => {
            json!({
                    "success": false, 
                    "error": format!("{}", e),
                }).to_string()
        }
    };
    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn get_state() -> *mut c_char {
    MobileApplicationManager::sync_wrapper(async_get_state())
}

async fn async_get_state() -> *mut c_char {
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let state = app_manager.get_state().await;
            json!({
                    "success": true, 
                    "message": { 
                        "state": state 
                    }
                }).to_string()
        },
        None => {
            json!({
                    "success": false, 
                    "error": "App manager is not initialized"
                }).to_string()
        }
    };
    
    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn generate_user_creds(vault_name_ptr: *const c_char) -> *mut c_char {
    let vault_name = c_str_to_string(vault_name_ptr);
    MobileApplicationManager::sync_wrapper(async_generate_user_creds(vault_name))
}

async fn async_generate_user_creds(vault_name: String) -> *mut c_char {
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            match app_manager.generate_user_creds(VaultName::from(vault_name)).await {
                Ok(app_state) => {
                    json!({
                        "success": true, 
                        "message": { 
                            "state": app_state 
                        }
                    }).to_string()
                }
                Err(e) => {
                    json!({
                        "success": false, 
                        "error": format!("{}", e)
                    }).to_string()
                }
            }
        },
        None => {
            json!({
                "success": false, 
                "error": "App manager is not initialized"
            }).to_string()
        }
    };

    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn sign_up() -> *mut c_char {
    MobileApplicationManager::sync_wrapper(async_sign_up())
}

async fn async_sign_up() -> *mut c_char {
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let state = match app_manager.sign_up().await {
                Ok(state) => {
                    json!({
                        "success": true, 
                        "message": { 
                            "state": state 
                        }
                    }).to_string()
                }
                Err(e) => {
                    json!({
                        "success": false, 
                        "error": format!("App manager is not initialized: {e}")
                    }).to_string()
                }
            };
            state
        },
        None => {
            json!({
                    "success": false, 
                    "error": "App manager is not initialized"
                }).to_string()
        }
    };

    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn update_membership(candidate_ptr: *const c_char, action_update_ptr: *const c_char) -> *mut c_char {
    let candidate = c_str_to_string(candidate_ptr);
    let action_update = c_str_to_string(action_update_ptr);
    MobileApplicationManager::sync_wrapper(async_update_membership(candidate, action_update))
}

async fn async_update_membership(candidate: String, action_update: String) -> *mut c_char {
    println!("🦀 Rust: candidate str {:?}", candidate);
    println!("🦀 Rust: action_update str {:?}", action_update);
    let candidate: UserData = match serde_json::from_str(&candidate) {
        Ok(data) => data,
        Err(e) => {
            return json_to_c_string(&json!({
                "success": false,
                "error": format!("Failed to parse a candidate: {}", e)
            }).to_string());
        }
    };
    println!("🦀 Rust: candidate {:?}", candidate);
    let join_action_update: JoinActionUpdate = match serde_json::from_str(&action_update) {
        Ok(data) => data,
        Err(e) => {
            return json_to_c_string(&json!({
                "success": false,
                "error": format!("Failed to parse a candidate: {}", e)
            }).to_string());
        }
    };
    println!("🦀 Rust: action_update {:?}", action_update);
    
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            app_manager.update_membership(candidate, join_action_update).await;
            json!({
                    "success": true
                }).to_string()
        },
        None => {
            json!({
                    "success": false, 
                    "error": "Update user join request is failed"
                }).to_string()
        }
    };

    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn clean_up_database() -> *mut c_char {
    MobileApplicationManager::sync_wrapper(async_clean_up_database())
}

async fn async_clean_up_database() -> *mut c_char {
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let state = app_manager.clean_up_database().await;
            json!({
                    "success": true
                }).to_string()
        },
        None => {
            json!({
                    "success": false, 
                    "error": "Cleaning up database failed"
                }).to_string()
        }
    };
    
    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

#[cfg(test)]
mod tests {
   
}
