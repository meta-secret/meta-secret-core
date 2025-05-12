use std::future::Future;

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use serde_json::json;
use std::sync::Arc;
use tracing::info;
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::common::model::vault::vault::{VaultName, VaultStatus};
use meta_secret_core::node::db::actions::sign_up::claim::SignUpClaim;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::objects::persistent_vault::PersistentVault;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;

fn sync_wrapper<F: Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    
    runtime.block_on(future)
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_sharedData_MetaSecretCoreServiceAndroid_00024NativeLib_sign_1up
(mut env: JNIEnv, _: JClass, user_name: JString) -> jstring {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("Calling sign_up function");
    
    let user_name_result = match env.get_string(&user_name) {
        Ok(s) => s.to_string_lossy().to_string(),
        Err(e) => {
            let error_json = json!({
                "success": false,
                "error": format!("Error converting JString: {}", e)
            }).to_string();
            
            match env.new_string(error_json) {
                Ok(s) => return s.into_raw(),
                Err(_) => return std::ptr::null_mut(),
            }
        }
    };
    
    info!("Received username: {}", user_name_result);

    let result: anyhow::Result<String> = sync_wrapper(async {
        info!("Starting sign_up async operation");
        
        let device_name = DeviceName::from(user_name_result.as_str());
        let vault_name = VaultName::from(user_name_result.as_str());
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));

        info!("Generating user credentials");
        let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
        let user_creds = p_creds
            .get_or_generate_user_creds(device_name.clone(), vault_name.clone())
            .await?;

        info!("Setting up HTTP sync protocol");
        let sync_protocol = HttpSyncProtocol {
            api_url: ApiUrl::prod(),
        };

        info!("Creating sync gateway");
        let client_gw = Arc::new(SyncGateway {
            id: "mobile_client".to_string(),
            p_obj: p_obj.clone(),
            sync: Arc::new(sync_protocol),
            device_creds: Arc::new(user_creds.device_creds.clone())
        });

        info!("First sync");
        client_gw.sync(user_creds.user()).await?;
        
        info!("Second sync");
        client_gw.sync(user_creds.user()).await?;

        info!("Performing sign_up operation");
        let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };
        sign_up_claim.sign_up(user_creds.user().clone()).await?;

        info!("Third sync");
        client_gw.sync(user_creds.user()).await?;
        
        info!("Fourth sync");
        client_gw.sync(user_creds.user()).await?;

        info!("Checking vault status");
        let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
        let vault_status = p_vault.find(user_creds.user().clone()).await?;

        info!("Status: {:?}", vault_status);
        
        match vault_status {
            VaultStatus::Member(member) => {
                info!("User is a vault member!");
                Ok(json!({
                    "success": true,
                    "status": "Member",
                    "data": {
                        "user_id": member.user_data.user_id(),
                        "device_id": member.user_data.device.device_id,
                        "vault_name": member.user_data.vault_name
                    }
                }).to_string())
            },
            other_status => {
                info!("Invalid vault status: {:?}", other_status);
                Ok(json!({
                    "success": true,
                    "status": format!("{:?}", other_status)
                }).to_string())
            }
        }
    });

    info!("Async operation completed, result: {:?}", result.is_ok());

    match result {
        Ok(json_str) => match env.new_string(json_str) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(err) => {
            let error_msg = err.to_string();
            info!("Error: {}", error_msg);
            
            let error_json = json!({
                "success": false,
                "error": error_msg
            }).to_string();
            
            match env.new_string(error_json) {
                Ok(s) => s.into_raw(),
                Err(_) => std::ptr::null_mut(),
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_sharedData_MetaSecretCoreServiceAndroid_00024NativeLib_free_1string
(_env: JNIEnv, _: JClass, _ptr: jstring) {
    info!("JNI free_string called");
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_sharedData_MetaSecretCoreServiceAndroid_00024NativeLib_get_1info
(mut env: JNIEnv, _: JClass) -> jstring {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("Calling get_info function");

    let result: anyhow::Result<String> = sync_wrapper(async {
        info!("Starting get_info async operation");
        
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));

        let p_creds = PersistentCredentials {
            p_obj: p_obj.clone(),
        };

        let maybe_device_creds = p_creds.get_device_creds().await?;
        if maybe_device_creds.is_none() {
            info!("Device not initialized");
            return Ok(json!({
                "success": true,
                "status": "NotInitialized",
                "message": "Device not initialized"
            }).to_string());
        }

        let device_creds = maybe_device_creds.unwrap().value();

        
        let maybe_user_creds = p_creds.get_user_creds().await?;
        if maybe_user_creds.is_none() {
            info!("Only device initialized, user missing");
            return Ok(json!({
                "success": true,
                "status": "DeviceOnly",
                "device": {
                    "id": device_creds.device.device_id,
                    "name": device_creds.device.device_name.as_str()
                }
            }).to_string());
        }

        let user_creds = maybe_user_creds.unwrap();

        let sync_protocol = HttpSyncProtocol {
            api_url: ApiUrl::prod(),
        };

        let client_gw = Arc::new(SyncGateway {
            id: "mobile_client".to_string(),
            p_obj: p_obj.clone(),
            sync: Arc::new(sync_protocol),
            device_creds: Arc::new(device_creds.clone()),
        });

        match client_gw.sync(user_creds.user()).await {
            Err(e) => {
                info!("Sync failed: {}", e);
                return Ok(json!({
                    "success": true,
                    "warning": format!("Sync failed: {}", e),
                    "status": "Offline",
                    "device": {
                        "id": device_creds.device.device_id,
                        "name": device_creds.device.device_name.as_str()
                    },
                    "user": {
                        "vault_name": user_creds.vault_name
                    }
                }).to_string());
            },
            _ => info!("Sync completed successfully")
        }
        
        let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
        let vault_status = p_vault.find(user_creds.user().clone()).await?;

        info!("Vault status: {:?}", vault_status);

        match vault_status {
            VaultStatus::Member(member) => {
                info!("User is a vault member");
                Ok(json!({
                    "success": true,
                    "status": "Member",
                    "device": {
                        "id": device_creds.device.device_id,
                        "name": device_creds.device.device_name.as_str()
                    },
                    "vault": {
                        "name": member.user_data.vault_name,
                        "owner_id": member.user_data.user_id().device_id.to_string()
                    }
                }).to_string())
            },
            VaultStatus::Outsider(outsider) => {
                info!("User is an outsider");
                Ok(json!({
                    "success": true,
                    "status": "Outsider",
                    "device": {
                        "id": device_creds.device.device_id,
                        "name": device_creds.device.device_name.as_str()
                    },
                    "vault": {
                        "name": outsider.user_data.vault_name()
                    }
                }).to_string())
            },
            VaultStatus::NotExists(user_data) => {
                info!("Vault does not exist");
                Ok(json!({
                    "success": true,
                    "status": "VaultNotExists",
                    "device": {
                        "id": device_creds.device.device_id,
                        "name": device_creds.device.device_name.as_str()
                    },
                    "vault": {
                        "name": user_data.vault_name()
                    }
                }).to_string())
            },
            _ => {
                info!("Unknown vault status");
                Ok(json!({
                    "success": true,
                    "status": "Unknown",
                    "device": {
                        "id": device_creds.device.device_id,
                        "name": device_creds.device.device_name.as_str()
                    },
                    "user": {
                        "vault_name": user_creds.vault_name
                    }
                }).to_string())
            }
        }
    });

    info!("get_info async operation completed, result: {:?}", result.is_ok());

    match result {
        Ok(json_str) => match env.new_string(json_str) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(err) => {
            let error_msg = err.to_string();
            info!("Error: {}", error_msg);
            
            let error_json = json!({
                "success": false,
                "error": error_msg
            }).to_string();
            
            match env.new_string(error_json) {
                Ok(s) => s.into_raw(),
                Err(_) => std::ptr::null_mut(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    #[ignore]
    fn test_sign_up_android() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .finish();
        let _guard_tracing = tracing::subscriber::set_global_default(subscriber).unwrap();

        let username = "test_android_user_1";
        
        info!("Test sign_up for user: '{}'", username);
        
        let result: anyhow::Result<String> = sync_wrapper(async {
            let device_name = DeviceName::from(username);
            let vault_name = VaultName::from(username);
            let repo = Arc::new(InMemKvLogEventRepo::default());
            let p_obj = Arc::new(PersistentObject::new(repo.clone()));

            info!("Generating user credentials");
            let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
            let user_creds = p_creds
                .get_or_generate_user_creds(device_name.clone(), vault_name.clone())
                .await?;

            info!("Setting up HTTP sync protocol");
            let sync_protocol = HttpSyncProtocol {
                api_url: ApiUrl::prod(),
            };

            info!("Creating sync gateway");
            let client_gw = Arc::new(SyncGateway {
                id: "mobile_client".to_string(),
                p_obj: p_obj.clone(),
                sync: Arc::new(sync_protocol),
                device_creds: Arc::new(user_creds.device_creds.clone())
            });

            info!("First sync");
            client_gw.sync(user_creds.user()).await?;
            
            info!("Second sync");
            client_gw.sync(user_creds.user()).await?;

            info!("Performing sign_up operation");
            let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };
            sign_up_claim.sign_up(user_creds.user().clone()).await?;

            info!("Third sync");
            client_gw.sync(user_creds.user()).await?;
            
            info!("Fourth sync");
            client_gw.sync(user_creds.user()).await?;

            info!("Checking vault status");
            let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
            let vault_status = p_vault.find(user_creds.user().clone()).await?;

            info!("Status: {:?}", vault_status);
            
            match vault_status {
                VaultStatus::Member(member) => {
                    info!("User is a vault member!");
                    Ok(json!({
                        "success": true,
                        "status": "Member",
                        "data": {
                            "user_id": member.user_data.user_id(),
                            "device_id": member.user_data.device.device_id,
                            "vault_name": member.user_data.vault_name
                        }
                    }).to_string())
                },
                other_status => {
                    info!("Invalid vault status: {:?}", other_status);
                    Ok(json!({
                        "success": true,
                        "status": format!("{:?}", other_status)
                    }).to_string())
                }
            }
        });

        match result {
            Ok(result_str) => {
                info!("Result: {}", result_str);

                let json_value: Value = serde_json::from_str(&result_str).expect("Invalid JSON");

                let success = json_value["success"]
                    .as_bool()
                    .expect("missing field: 'success'");
                info!("Success: {}", success);

                if success {
                    if let Some(status) = json_value["status"].as_str() {
                        info!("Status: {}", status);

                        if status == "Member" {
                            let data = &json_value["data"];
                            info!("User information:");
                            info!("  User ID: {}", data["user_id"].as_str().unwrap_or("N/A"));
                            info!(
                                "  Device ID: {}",
                                data["device_id"].as_str().unwrap_or("N/A")
                            );
                            info!(
                                "  Vault name: {}",
                                data["vault_name"].as_str().unwrap_or("N/A")
                            );
                        } else {
                            info!("User is not a member. Status: {}", status);
                        }
                    }
                } else {
                    if let Some(error) = json_value["error"].as_str() {
                        info!("Error: {}", error);
                    }
                }
            },
            Err(e) => {
                info!("Error during execution: {}", e);
                panic!("Test failed with error: {}", e);
            }
        }
    }

    #[test]
    #[ignore]
    fn test_get_info_android() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .finish();
        let _guard_tracing = tracing::subscriber::set_global_default(subscriber).unwrap();

        info!("Test get_info");
        
        let result: anyhow::Result<String> = sync_wrapper(async {
            let repo = Arc::new(InMemKvLogEventRepo::default());
            let p_obj = Arc::new(PersistentObject::new(repo.clone()));

            let p_creds = PersistentCredentials {
                p_obj: p_obj.clone(),
            };
            
            let maybe_device_creds = p_creds.get_device_creds().await?;
            
            if maybe_device_creds.is_none() {
                info!("Device not initialized");
                return Ok(json!({
                    "success": true,
                    "status": "NotInitialized",
                    "message": "Device not initialized"
                }).to_string());
            }

            let device_creds = maybe_device_creds.unwrap().value();
            
            let maybe_user_creds = p_creds.get_user_creds().await?;
            
            if maybe_user_creds.is_none() {
                info!("Only device initialized, user missing");
                return Ok(json!({
                    "success": true,
                    "status": "DeviceOnly",
                    "device": {
                        "id": device_creds.device.device_id,
                        "name": device_creds.device.device_name.as_str()
                    }
                }).to_string());
            }

            let user_creds = maybe_user_creds.unwrap();
            
            let sync_protocol = HttpSyncProtocol {
                api_url: ApiUrl::prod(),
            };

            let client_gw = Arc::new(SyncGateway {
                id: "mobile_client".to_string(),
                p_obj: p_obj.clone(),
                sync: Arc::new(sync_protocol),
                device_creds: Arc::new(device_creds.clone()),
            });
            
            match client_gw.sync(user_creds.user()).await {
                Err(e) => {
                    info!("Sync failed: {}", e);
                    return Ok(json!({
                        "success": true,
                        "warning": format!("Sync failed: {}", e),
                        "status": "Offline",
                        "device": {
                            "id": device_creds.device.device_id,
                            "name": device_creds.device.device_name.as_str()
                        },
                        "user": {
                            "vault_name": user_creds.vault_name
                        }
                    }).to_string());
                },
                _ => info!("Sync completed successfully")
            }
            
            let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
            let vault_status = p_vault.find(user_creds.user().clone()).await?;

            info!("Vault status: {:?}", vault_status);
            match vault_status {
                VaultStatus::Member(member) => {
                    info!("User is a vault member");
                    Ok(json!({
                        "success": true,
                        "status": "Member",
                        "device": {
                            "id": device_creds.device.device_id,
                            "name": device_creds.device.device_name.as_str()
                        },
                        "vault": {
                            "name": member.user_data.vault_name,
                            "owner_id": member.user_data.user_id().device_id.to_string()
                        }
                    }).to_string())
                },
                VaultStatus::Outsider(outsider) => {
                    info!("User is an outsider");
                    Ok(json!({
                        "success": true,
                        "status": "Outsider",
                        "device": {
                            "id": device_creds.device.device_id,
                            "name": device_creds.device.device_name.as_str()
                        },
                        "vault": {
                            "name": outsider.user_data.vault_name()
                        }
                    }).to_string())
                },
                VaultStatus::NotExists(user_data) => {
                    info!("Vault does not exist");
                    Ok(json!({
                        "success": true,
                        "status": "VaultNotExists",
                        "device": {
                            "id": device_creds.device.device_id,
                            "name": device_creds.device.device_name.as_str()
                        },
                        "vault": {
                            "name": user_data.vault_name()
                        }
                    }).to_string())
                },
                _ => {
                    info!("Unknown vault status");
                    Ok(json!({
                        "success": true,
                        "status": "Unknown",
                        "device": {
                            "id": device_creds.device.device_id,
                            "name": device_creds.device.device_name.as_str()
                        },
                        "user": {
                            "vault_name": user_creds.vault_name
                        }
                    }).to_string())
                }
            }
        });

        match result {
            Ok(result_str) => {
                info!("Result: {}", result_str);

                let json_value: Value = serde_json::from_str(&result_str).expect("Invalid JSON");

                let success = json_value["success"]
                    .as_bool()
                    .expect("missing field: 'success'");
                info!("Success: {}", success);

                if success {
                    if let Some(status) = json_value["status"].as_str() {
                        info!("Status: {}", status);

                        match status {
                            "NotInitialized" => {
                                info!("Device not initialized");
                            },
                            "DeviceOnly" => {
                                let device = &json_value["device"];
                                info!("Device information:");
                                info!("  Device ID: {}", device["id"].as_str().unwrap_or("N/A"));
                                info!("  Device name: {}", device["name"].as_str().unwrap_or("N/A"));
                            },
                            "Member" => {
                                let device = &json_value["device"];
                                let vault = &json_value["vault"];
                                
                                info!("Device information:");
                                info!("  Device ID: {}", device["id"].as_str().unwrap_or("N/A"));
                                info!("  Device name: {}", device["name"].as_str().unwrap_or("N/A"));
                                
                                info!("Vault information:");
                                info!("  Vault name: {}", vault["name"].as_str().unwrap_or("N/A"));
                                info!("  Owner ID: {}", vault["owner_id"].as_str().unwrap_or("N/A"));
                                
                                if let Some(users) = vault["users"].as_array() {
                                    info!("Users in vault:");
                                    for user in users {
                                        let user_type = user["type"].as_str().unwrap_or("Unknown");
                                        let device_id = user["device_id"].as_str().unwrap_or("N/A");
                                        if user_type == "Member" {
                                            let device_name = user["device_name"].as_str().unwrap_or("N/A");
                                            info!("  Member: {} ({})", device_name, device_id);
                                        } else {
                                            info!("  Outsider: {}", device_id);
                                        }
                                    }
                                }
                                
                                if let Some(secrets) = vault["secrets"].as_array() {
                                    info!("Secrets in vault:");
                                    for secret in secrets {
                                        let secret_id = secret["id"].as_str().unwrap_or("N/A");
                                        let secret_name = secret["name"].as_str().unwrap_or("N/A");
                                        info!("  Secret: {} ({})", secret_name, secret_id);
                                    }
                                }
                            },
                            _ => {
                                info!("Other status: {}", status);
                            }
                        }
                    }
                } else {
                    if let Some(error) = json_value["error"].as_str() {
                        info!("Error: {}", error);
                    }
                }
            },
            Err(e) => {
                info!("Error during execution: {}", e);
                panic!("Test failed with error: {}", e);
            }
        }
    }
}

