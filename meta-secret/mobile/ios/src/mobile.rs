use std::ffi::{CString};
use std::os::raw::c_char;
use meta_secret_core::crypto::key_pair::MasterKeyManager;
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};
use std::future::Future;
use serde_json::json;
use mobile_common::mobile_manager::MobileApplicationManager;

// MARK: Helpers
static APP_MANAGER: Lazy<Mutex<Option<Arc<MobileApplicationManager>>>> =
    Lazy::new(|| Mutex::new(None));

static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| { 
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
    });

fn sync_wrapper<F: Future>(future: F) -> F::Output {
    RUNTIME.block_on(future)
}

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
    sync_wrapper(async_generate_master_key())
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
    sync_wrapper(async_init(master_key))
}

async fn async_init(master_key: String) -> *mut c_char {
    let transport_sk = MasterKeyManager::from_pure_sk(master_key);
    match MobileApplicationManager::init_ios(transport_sk).await {
        Ok(app_manager) => {
            let mut global = APP_MANAGER.lock().unwrap();
            *global = Some(Arc::new(app_manager));
            
            json_to_c_string(r#"{"success": true, "message":"ios success"}"#)
        },
        Err(e) => {
            json_to_c_string(&format!(r#"{{"success": false, "message":"{e}"}}"#))
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn get_state() -> *mut c_char {
    sync_wrapper(async_get_state())
}

async fn async_get_state() -> *mut c_char {
    let app_manager_lock = APP_MANAGER.lock().unwrap();
    
    match &*app_manager_lock {
        Some(app_manager) => {
            json_to_c_string(r#"{"success": true, "message": "App manager is initialized"}"#)
        },
        None => {
            json_to_c_string(r#"{"success": false, "message": "App manager not initialized"}"#)
        }
    }
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
