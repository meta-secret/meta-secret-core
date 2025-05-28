use std::future::Future;
use std::sync::{Arc, Mutex};
use meta_secret_core::crypto::key_pair::MasterKeyManager;
use once_cell::sync::Lazy;
use mobile_common::mobile_manager::MobileApplicationManager;
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use serde_json::json;

// MARK: Helpers
static APP_MANAGER: Lazy<Mutex<Option<Arc<MobileApplicationManager>>>> =
    Lazy::new(|| Mutex::new(None));

static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
});

fn sync_wrapper<F: Future>(future: F) -> F::Output { 
    RUNTIME.block_on(future)
}

fn rust_to_java_string(env: &mut JNIEnv, rust_string: String) -> jstring {
    match env.new_string(rust_string) {
        Ok(jstr) => jstr.into_raw(),
        Err(_) => {
        // Return an error JSON string as fallback
            let error_msg = r#"{"success":false,"message":"Failed to create Java string"}"#;
            env.new_string(error_msg)
                .unwrap_or_else(|_| env.new_string("").unwrap())
                .into_raw()
        }
    }
}

fn java_to_rust_string(env: &mut JNIEnv, java_string: JString) -> Result<String, String> {
    env.get_string(&java_string)
        .map(|s| s.into())
        .map_err(|e| format!("Failed to convert Java string: {}", e))
}

// MARK: Generating / init
#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_generateMasterKey(
    mut env: JNIEnv, 
    _: JClass
) -> jstring {
    let result = sync_wrapper(async {
        let generated_master_key = MasterKeyManager::generate_sk();
        json!({
            "success": true,
            "message": generated_master_key
        }).to_string()
    });
    
    rust_to_java_string(&mut env, result)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_init(
    mut env: JNIEnv, 
    _: JClass, 
    master_key_jstring: JString
) -> jstring {
    let master_key = match java_to_rust_string(&mut env, master_key_jstring) {
        Ok(key) => key,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "message": e
            }).to_string());
        }
    };
    
    let result = sync_wrapper(async move {
        let transport_sk = MasterKeyManager::from_pure_sk(master_key);
        match MobileApplicationManager::init_android(transport_sk).await {
            Ok(app_manager) => {
                let mut global = APP_MANAGER.lock().unwrap();
                *global = Some(Arc::new(app_manager));
                
                json!({
                    "success": true, 
                    "message": "Android manager initialized successfully"
                }).to_string()
            },
            Err(e) => {
                json!({
                    "success": false, 
                    "message": format!("Error: {}", e)
                }).to_string()
            }
        }
    });
    
    rust_to_java_string(&mut env, result)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_getState(
    mut env: JNIEnv, 
    _: JClass
) -> jstring {
    let result = sync_wrapper(async {
        let app_manager_lock = APP_MANAGER.lock().unwrap();
        
        match &*app_manager_lock {
            Some(app_manager) => {
                let state = app_manager.get_state().await;
                json!({
                    "success": true, 
                    "state": {
                        "isInitialized": true,
                        "state": "App manager is initialized"
                    }
                }).to_string()
            },
            None => {
                json!({
                    "success": false, 
                    "message": "App manager not initialized"
                }).to_string()
            }
        }
    });
    
    rust_to_java_string(&mut env, result)
}

#[cfg(test)]
mod tests {
   
}