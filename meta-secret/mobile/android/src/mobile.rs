use std::os::raw::c_char;
use meta_secret_core::crypto::key_pair::MasterKeyManager;
use mobile_common::mobile_manager::MobileApplicationManager;
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use serde_json::json;
use std::sync::Arc;
use meta_secret_core::crypto::utils::Id48bit;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::ClaimId;
use meta_secret_core::node::common::model::user::common::UserData;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::actions::sign_up::join::JoinActionUpdate;

fn rust_to_java_string(env: &mut JNIEnv, rust_string: String) -> jstring {
    match env.new_string(rust_string) {
        Ok(jstr) => jstr.into_raw(),
        Err(_) => {
            let error_msg = r#"{"success":false,"error":"Failed to create Java string"}"#;
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
    let result = MobileApplicationManager::sync_wrapper(async {
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
    master_key: JString
) -> jstring {
    let master_key = match java_to_rust_string(&mut env, master_key) {
        Ok(key) => key,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": e
            }).to_string());
        }
    };
    
    let result = MobileApplicationManager::sync_wrapper(async move {
        let transport_sk = MasterKeyManager::from_pure_sk(master_key);
        match MobileApplicationManager::init_android(transport_sk).await {
            Ok(app_manager) => {
                MobileApplicationManager::set_global_instance(Arc::new(app_manager));
                json!({
                    "success": true, 
                    "message": "Android manager initialized successfully"
                }).to_string()
            },
            Err(e) => {
                json!({
                    "success": false, 
                    "error": format!("{}", e)
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
    let result = MobileApplicationManager::sync_wrapper(async {
        match MobileApplicationManager::get_global_instance() {
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
        }
    });
    
    rust_to_java_string(&mut env, result)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_signUp(
    mut env: JNIEnv,
    _: JClass
) -> jstring {
    let result = MobileApplicationManager::sync_wrapper(async {
        match MobileApplicationManager::get_global_instance() {
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
                            "error": format!("SignUp is failed {e}")
                        }).to_string()
                    }
                };
                state
            },
            None => {
                json!({
                    "success": false, 
                    "error": "SignUp is failed"
                }).to_string()
            }
        }
    });

    rust_to_java_string(&mut env, result)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_generate_user_creds(
    mut env: JNIEnv,
    _: JClass,
    vault_name: JString
) -> jstring {
    let vault_name_string = match java_to_rust_string(&mut env, vault_name) {
        Ok(res) => res,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": e
            }).to_string());
        }
    };
    
    let result = MobileApplicationManager::sync_wrapper(async {
        match MobileApplicationManager::get_global_instance() {
            Some(app_manager) => {
                match app_manager.generate_user_creds(VaultName::from(vault_name_string)).await {
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
        }
    });

    rust_to_java_string(&mut env, result)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_update_membership(
    mut env: JNIEnv,
    _: JClass,
    candidate_ptr: JString,
    action_update_ptr: JString
) -> jstring {
    let candidate = match java_to_rust_string(&mut env, candidate_ptr) {
        Ok(res) => res,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": e
            }).to_string());
        }
    };

    let action_update = match java_to_rust_string(&mut env, action_update_ptr) {
        Ok(res) => res,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": e
            }).to_string());
        }
    };

    let candidate: UserData = match serde_json::from_str(&candidate) {
        Ok(data) => data,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": format!("Failed to parse candidate: {}", e)
            }).to_string());
        }
    };
    println!("🦀 Rust: candidate {:?}", candidate);
    let action_update: JoinActionUpdate = match serde_json::from_str(&action_update) {
        Ok(data) => data,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": format!("Failed to parse action update: {}", e)
            }).to_string());
        }
    };
    println!("🦀 Rust: action_update {:?}", action_update);

    let result = MobileApplicationManager::sync_wrapper(async {
        match MobileApplicationManager::get_global_instance() {
            Some(app_manager) => {
                let result = match app_manager.update_membership(candidate, action_update).await {
                    Ok(_) => {
                        json!({
                            "success": true,
                        }).to_string()
                    }
                    Err(e) => {
                        json!({
                            "success": false,
                            "error": format!("Failed to parse action update: {}", e)
                        }).to_string()
                    }
                };
                result
            },
            None => {
                json!({
                    "success": false,
                    "error": "App manager is not initialized"
                }).to_string()
            }
        }
    });
    
    rust_to_java_string(&mut env, result)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_clean_up_database(
    mut env: JNIEnv,
    _: JClass
) -> jstring {
    let result = MobileApplicationManager::sync_wrapper(async {
        match MobileApplicationManager::get_global_instance() {
            Some(app_manager) => {
                app_manager.clean_up_database().await;
                json!({ "success": true}).to_string()
            },
            None => {
                json!({
                    "success": false,
                    "error": "App manager is not initialized"
                }).to_string()
            }
        }
    });

    rust_to_java_string(&mut env, result)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_split(
    mut env: JNIEnv,
    _: JClass,
    pass_id: JString,
    pass: JString
) -> jstring {
    let pass_id = match java_to_rust_string(&mut env, pass_id) {
        Ok(pass_id) => pass_id,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": e
            }).to_string());
        }
    };

    let pass = match java_to_rust_string(&mut env, pass) {
        Ok(pass) => pass,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": e
            }).to_string());
        }
    };

    let plain_pass_info = PlainPassInfo::new(pass_id, pass);

    let result = MobileApplicationManager::sync_wrapper(async {
        match MobileApplicationManager::get_global_instance() {
            Some(app_manager) => {
                app_manager.cluster_distribution(&plain_pass_info).await;
                json!({
                    "success": true,
                }).to_string()
            },
            None => {
                json!({
                    "success": false,
                    "error": "Distribution is failed"
                }).to_string()
            }
        }
    });

    rust_to_java_string(&mut env, result)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_find_claim_by_(
    mut env: JNIEnv,
    _: JClass,
    secret_id: JString,
) -> jstring {
    let secret_id = match java_to_rust_string(&mut env, secret_id) {
        Ok(key) => key,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": e
            }).to_string());
        }
    };

    let result = MobileApplicationManager::sync_wrapper(async {
        match MobileApplicationManager::get_global_instance() {
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
                    "error": "App manager is not initialized"
                }).to_string()
            }
        }
    });

    rust_to_java_string(&mut env, result)
}


#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_recover(
    mut env: JNIEnv,
    _: JClass,
    secret_id: JString,
) -> jstring {
    let secret_id = match java_to_rust_string(&mut env, secret_id) {
        Ok(pass_id) => pass_id,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": e
            }).to_string());
        }
    };

    let meta_password_id = MetaPasswordId::build(secret_id);

    let result = MobileApplicationManager::sync_wrapper(async {
        match MobileApplicationManager::get_global_instance() {
            Some(app_manager) => {
                app_manager.recover(&meta_password_id).await;
                json!({
                    "success": true,
                }).to_string()
            },
            None => {
                json!({
                    "success": false,
                    "error": "Recover request is failed"
                }).to_string()
            }
        }
    });

    rust_to_java_string(&mut env, result)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_acceptRecover(
    mut env: JNIEnv,
    _: JClass,
    claim_id: JString,
) -> jstring {
    let claim_id = match java_to_rust_string(&mut env, claim_id) {
        Ok(pass_id) => pass_id,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": e
            }).to_string());
        }
    };

    let meta_claim_id = ClaimId::from(Id48bit::from(claim_id));

    let result = MobileApplicationManager::sync_wrapper(async {
        match MobileApplicationManager::get_global_instance() {
            Some(app_manager) => {
                app_manager.accept_recover(meta_claim_id).await;
                json!({
                    "success": true,
                }).to_string()
            },
            None => {
                json!({
                    "success": false,
                    "error": "Recover request is failed"
                }).to_string()
            }
        }
    });

    rust_to_java_string(&mut env, result)
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_metasecret_core_MetaSecretNative_showRecovered(
    mut env: JNIEnv,
    _: JClass,
    secret_id: JString,
) -> jstring {
    let secret_id = match java_to_rust_string(&mut env, secret_id) {
        Ok(pass_id) => pass_id,
        Err(e) => {
            return rust_to_java_string(&mut env, json!({
                "success": false,
                "error": e
            }).to_string());
        }
    };

    let meta_password_id = MetaPasswordId::build_from_str(&secret_id);

    let result = MobileApplicationManager::sync_wrapper(async {
        match MobileApplicationManager::get_global_instance() {
            Some(app_manager) => {
                let secret =  app_manager.show_recovered(&meta_password_id).await;
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
                    "error": "Recover request is failed"
                }).to_string()
            }
        }
    });

    rust_to_java_string(&mut env, result)
}

#[cfg(test)]
mod tests {
   
}