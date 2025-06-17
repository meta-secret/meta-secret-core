use std::ffi::{CString};
use std::os::raw::c_char;
use meta_secret_core::crypto::key_pair::MasterKeyManager;
use serde_json::json;
use mobile_common::mobile_manager::MobileApplicationManager;
use std::sync::Arc;

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
    let key_len = master_key.len();
    let diagnostic_info = format!("Key length: {}", key_len);
    
    let transport_sk = MasterKeyManager::from_pure_sk(master_key);
    
    let pk_result = match transport_sk.pk() {
        Ok(_) => "Valid public key generated".to_string(),
        Err(e) => format!("Invalid public key: {}", e),
    };
    
    let result = match MobileApplicationManager::init_ios(transport_sk).await {
        Ok(app_manager) => {
            MobileApplicationManager::set_global_instance(Arc::new(app_manager));
            json!({
                    "success": true,
                    "message": "iOS manager initialized successfully",
                    "debug_info": {
                        "key_diagnostic": diagnostic_info,
                        "pk_validation": pk_result
                    }
                }).to_string()
        },
        Err(e) => {
            json!({
                    "success": false, 
                    "error": format!("{}", e),
                    "debug_info": {
                        "key_diagnostic": diagnostic_info,
                        "pk_validation": pk_result
                    }
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
