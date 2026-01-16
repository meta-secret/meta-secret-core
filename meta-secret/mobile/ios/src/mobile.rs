use std::ffi::{CString};
use std::os::raw::c_char;
use meta_secret_core::crypto::key_pair::MasterKeyManager;
use serde_json::json;
use mobile_common::mobile_manager::MobileApplicationManager;
use std::sync::Arc;
use meta_secret_core::crypto::utils::Id48bit;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::ClaimId;
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
    println!("🦀 Mobile async_init ");
    let transport_sk = MasterKeyManager::from_pure_sk(master_key.clone());
    
    let result = match MobileApplicationManager::init_ios(transport_sk, master_key).await {
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
            let state = match app_manager.get_state().await {
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
                    "error": format!("App manager is not initialized {e}")
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
    println!("🦀 Mobile iOS API: candidate str {:?}", candidate);
    println!("🦀 Mobile iOS API: action_update str {:?}", action_update);
    let candidate: UserData = match serde_json::from_str(&candidate) {
        Ok(data) => data,
        Err(e) => {
            return json_to_c_string(&json!({
                "success": false,
                "error": format!("Failed to parse a candidate: {}", e)
            }).to_string());
        }
    };
    println!("🦀 Mobile iOS API: candidate {:?}", candidate);
    let join_action_update: JoinActionUpdate = match serde_json::from_str(&action_update) {
        Ok(data) => data,
        Err(e) => {
            return json_to_c_string(&json!({
                "success": false,
                "error": format!("Failed to parse a candidate: {}", e)
            }).to_string());
        }
    };
    println!("🦀 Mobile iOS API: action_update {:?}", action_update);
    
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let res = match app_manager.update_membership(candidate, join_action_update).await {
                Ok(_) => {
                    json!({
                        "success": true
                    }).to_string()
                }
                Err(e) => {
                    json!({
                        "success": false,
                        "error": format!("Update user join request is failed {e}")
                    }).to_string()
                }
            };
            res
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
            app_manager.clean_up_database().await;
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
pub extern "C" fn split_secret(secret_id_ptr: *const c_char, secret_ptr: *const c_char) -> *mut c_char {
    let secret_id = c_str_to_string(secret_id_ptr);
    let secret = c_str_to_string(secret_ptr);
    MobileApplicationManager::sync_wrapper(async_split_secret(secret_id, secret))
}

async fn async_split_secret(secret_id: String, secret_ptr: String) -> *mut c_char {
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_pass_id = MetaPasswordId::build_from_str(&secret_id);
            println!("🦀 Mobile iOS API: split secret meta_pass_id {:?}", meta_pass_id);

            let plan_pass_info = PlainPassInfo {
                pass_id: meta_pass_id,
                pass: secret_ptr,
            };

            app_manager.cluster_distribution(&plan_pass_info).await;
            json!({
                    "success": true
                }).to_string()
        },
        None => {
            json!({
                "success": false,
                "error": "Secret split request is failed"
            }).to_string()
        }
    };

    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn find_claim_by(secret_id_ptr: *const c_char) -> *mut c_char {
    let secret_id = c_str_to_string(secret_id_ptr);
    MobileApplicationManager::sync_wrapper(async_find_claim_by(secret_id))
}

async fn async_find_claim_by(secret_id: String) -> *mut c_char {
    println!("🦀 Mobile iOS API: find claim by {:?}", secret_id);
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_password_id = MetaPasswordId::build_from_str(&secret_id);

            let res = match app_manager.find_claim_by_pass_id(&meta_password_id).await {
                Some(claim) => {
                    json!({
                        "success": true,
                        "message": {
                            "claim": claim
                        }
                    }).to_string()
                }
                None => {
                    json!({
                        "success": false,
                        "error": "Update find claim request is failed".to_string()
                    }).to_string()
                }
            };
            res
        },
        None => {
            json!({
                "success": false,
                "error": "Find claim request is failed"
            }).to_string()
        }
    };

    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn recover(secret_id_ptr: *const c_char) -> *mut c_char {
    let secret_id = c_str_to_string(secret_id_ptr);
    MobileApplicationManager::sync_wrapper(async_recover(secret_id))
}

async fn async_recover(secret_id: String) -> *mut c_char {
    println!("🦀 Mobile iOS API: recover secret_id {:?}", secret_id);
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_password_id = MetaPasswordId::build_from_str(&secret_id);
            println!("🦀 Mobile iOS API: recover meta_password_id {:?}", meta_password_id);
            app_manager.recover(&meta_password_id).await;
            json!({
                    "success": true
                }).to_string()
        },
        None => {
            json!({
                "success": false,
                "error": "Recover request is failed"
            }).to_string()
        }
    };

    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn accept_recover(claim_id_ptr: *const c_char) -> *mut c_char {
    let claim_id = c_str_to_string(claim_id_ptr);
    MobileApplicationManager::sync_wrapper(async_accept_recover(claim_id))
}

async fn async_accept_recover(claim_id: String) -> *mut c_char {
    println!("🦀 Mobile iOS API: accept recover claim {:?}", claim_id);
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_claim_id = ClaimId::from(Id48bit::from(claim_id));
            println!("🦀 Mobile iOS API: meta_claim_id {:?}", meta_claim_id);

            match app_manager.accept_recover_mobile(meta_claim_id).await {
                Ok(_) => {
                    json!({
                        "success": true
                    }).to_string()
                }
                Err(e) => {
                    println!("🦀 Mobile iOS API: accept recover failed: {}", e);
                    json!({
                        "success": false,
                        "error": format!("Accept recover failed: {}", e)
                    }).to_string()
                }
            }
        },
        None => {
            json!({
                "success": false,
                "error": "Accept recover request is failed"
            }).to_string()
        }
    };

    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn decline_recover(claim_id_ptr: *const c_char) -> *mut c_char {
    let claim_id = c_str_to_string(claim_id_ptr);
    MobileApplicationManager::sync_wrapper(async_decline_recover(claim_id))
}

async fn async_decline_recover(claim_id: String) -> *mut c_char {
    println!("🦀 Mobile iOS API: decline recover claim {:?}", claim_id);
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_claim_id = ClaimId::from(Id48bit::from(claim_id));
            println!("🦀 Mobile iOS API: decline meta_claim_id {:?}", meta_claim_id);

            match app_manager.decline_recover_mobile(meta_claim_id).await {
                Ok(_) => {
                    json!({
                        "success": true
                    }).to_string()
                }
                Err(e) => {
                    println!("🦀 Mobile iOS API: decline recover failed: {}", e);
                    json!({
                        "success": false,
                        "error": format!("Decline recover failed: {}", e)
                    }).to_string()
                }
            }
        },
        None => {
            json!({
                "success": false,
                "error": "Decline recover request is failed"
            }).to_string()
        }
    };

    json_to_c_string(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn show_recovered(secret_id_ptr: *const c_char) -> *mut c_char {
    let secret_id = c_str_to_string(secret_id_ptr);
    MobileApplicationManager::sync_wrapper(async_show_recovered(secret_id))
}

async fn async_show_recovered(secret_id: String) -> *mut c_char {
    println!("🦀 Mobile iOS API: show recovered by pass id {:?}", secret_id);
    let result = match MobileApplicationManager::get_global_instance() {
        Some(app_manager) => {
            let meta_password_id = MetaPasswordId::build_from_str(&secret_id);

            let secret = app_manager.show_recovered(&meta_password_id).await;
            json!({
                "success": true,
                "message": {
                    "secret": secret
                }
            }).to_string()
        },
        None => {
            json!({
                "success": false,
                "error": "Show recovered request is failed"
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
