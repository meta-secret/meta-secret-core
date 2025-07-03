use std::ffi::{CString};
use std::os::raw::c_char;
use meta_secret_core::crypto::key_pair::MasterKeyManager;
use serde_json::json;
use mobile_common::mobile_manager::MobileApplicationManager;
use std::sync::Arc;
use meta_secret_core::node::common::model::vault::vault::VaultName;

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
            let state = app_manager.get_state().await.as_info();
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
    println!("🦀 Rust: async_generate_user_creds with {:?}", vault_name);
    
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            match app_manager.generate_user_creds(VaultName::from(vault_name)).await {
                Ok(app_state) => {
                    println!("🦀 Rust: Vault created with {:?}", app_state);
                    
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
            let state = app_manager.sign_up().await.as_info();
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
pub extern "C" fn clean_up_database() -> *mut c_char {
    MobileApplicationManager::sync_wrapper(async_clean_up_database())
}

async fn async_clean_up_database() -> *mut c_char {
    

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
