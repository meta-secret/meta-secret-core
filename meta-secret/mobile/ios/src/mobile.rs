use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use anyhow::anyhow;
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
use serde_json::json;
use std::sync::Arc;
use tracing::info;
use meta_secret_core::crypto::key_pair::{KeyPair, TransportDsaKeyPair};

fn sync_wrapper<F: Future>(future: F) -> F::Output {
    // Создаем однопоточный токио рантайм
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    
    // Выполняем асинхронную операцию в рантайме
    rt.block_on(future)
}

#[unsafe(no_mangle)]
pub extern "C" fn sign_up(user_name: *const c_char) -> *mut c_char {
    sync_wrapper(async_sign_up(user_name))
}

async extern "C" fn async_sign_up(user_name: *const c_char) -> *mut c_char {
    if user_name.is_null() {
        let error_json = json!({
            "success": false,
            "error": "Null user_name pointer provided"
        })
        .to_string();
        return CString::new(error_json).unwrap_or_default().into_raw();
    }

    // let result: anyhow::Result<String> = sync_wrapper(async {
    let user_name_str = unsafe { CStr::from_ptr(user_name) }
        .to_str()
        .map_err(|e| anyhow!("Invalid UTF-8 string: {}", e))
        .unwrap();
    let device_name = DeviceName::from(user_name_str);
    let vault_name = VaultName::from(user_name_str);
    let repo = Arc::new(InMemKvLogEventRepo::default());
    let p_obj = Arc::new(PersistentObject::new(repo.clone()));

    let master_key = TransportDsaKeyPair::generate().sk();

    let p_creds = PersistentCredentials {
        p_obj: p_obj.clone(),
        master_key: master_key.clone()
    };
    let user_creds = p_creds
        .get_or_generate_user_creds(device_name.clone(), vault_name.clone())
        .await
        .unwrap();

    let sync_protocol = HttpSyncProtocol {
        api_url: ApiUrl::dev(),
    };

    let client_gw = Arc::new(SyncGateway {
        id: "mobile_client".to_string(),
        p_obj: p_obj.clone(),
        sync: Arc::new(sync_protocol),
        master_key,
    });

    client_gw.sync(user_creds.user()).await.unwrap();
    client_gw.sync(user_creds.user()).await.unwrap();

    let sign_up_claim = SignUpClaim {
        p_obj: p_obj.clone(),
    };
    sign_up_claim
        .sign_up(user_creds.user().clone())
        .await
        .unwrap();

    client_gw.sync(user_creds.user()).await.unwrap();
    client_gw.sync(user_creds.user()).await.unwrap();

    let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
    let vault_status = p_vault.find(user_creds.user().clone()).await.unwrap();

    let result: Result<String, String> = match vault_status {
        VaultStatus::Member(member) => {
            info!("The user is a vault member!");
            Ok(json!({
                "success": true,
                "status": "Member",
                "data": {
                    "user_id": member.user_data.user_id(),
                    "device_id": member.user_data.device.device_id,
                    "vault_name": member.user_data.vault_name
                }
            })
            .to_string())
        }
        other_status => {
            info!("Invalid storage status: {:?}", other_status);
            Ok(json!({
                "success": true,
                "status": format!("{:?}", other_status)
            })
            .to_string())
        }
    };

    match result {
        Ok(json_str) => CString::new(json_str).unwrap_or_default().into_raw(),
        Err(err) => {
            let error_json = json!({
                "success": false,
                "error": err.to_string()
            })
            .to_string();
            CString::new(error_json).unwrap_or_default().into_raw()
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
    use super::*;
    use meta_secret_core::node::app::sync::api_url::ApiUrl;
    use serde_json::Value;
    use std::time::Duration;

    #[tokio::test]
    #[ignore]
    async fn test_sign_up_debug() {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .finish();
        let _guard = tracing::subscriber::set_global_default(subscriber).unwrap();

        use reqwest::Client;
        let client = Client::new();
        let url = ApiUrl::dev().get_url() + "/meta_request";

        let request_builder = client
            .get(url.as_str())
            .timeout(Duration::from_secs(3))
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", url.as_str());

        info!("Request: {:?}", request_builder);

        let response = request_builder.send().await;
        info!("Response: {:?}", response);
        let username = "test_debug_user_1";
        let c_username = CString::new(username).unwrap();

        info!("Execute sign_up for user: '{}'", username);
        let result_ptr = sign_up(c_username.as_ptr());

        let result_str = unsafe {
            assert!(!result_ptr.is_null(), "sign_up - null pointer");
            let result_cstr = CStr::from_ptr(result_ptr);
            let result_string = result_cstr.to_str().expect("Invalid UTF-8").to_owned();

            free_string(result_ptr);

            result_string
        };

        info!("Got result: {}", result_str);

        let json_value: Value = serde_json::from_str(&result_str).expect("Invalid JSON");

        let success = json_value["success"]
            .as_bool()
            .expect("missing filed: 'success'");
        info!("Success: {}", success);

        if success {
            if let Some(status) = json_value["status"].as_str() {
                info!("Status: {}", status);

                if status == "Member" {
                    let data = &json_value["data"];
                    info!("User info:");
                    info!("  User ID: {}", data["user_id"].as_str().unwrap_or("N/A"));
                    info!(
                        "  Device ID: {}",
                        data["device_id"].as_str().unwrap_or("N/A")
                    );
                    info!(
                        "  Vault Name: {}",
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
    }

    #[test]
    #[ignore]
    fn test_sign_up_sync() {
        // Инициализируем токио рантайм
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

        let username = "test_sync_user_1";
        let c_username = CString::new(username).unwrap();

        info!("Execute sign_up_sync for user: '{}'", username);
        let result_ptr = sign_up(c_username.as_ptr());

        let result_str = unsafe {
            assert!(!result_ptr.is_null(), "sign_up_sync - null pointer");
            let result_cstr = CStr::from_ptr(result_ptr);
            let result_string = result_cstr.to_str().expect("Invalid UTF-8").to_owned();

            free_string(result_ptr);

            result_string
        };

        info!("Got result: {}", result_str);

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
                    info!("User info:");
                    info!("  User ID: {}", data["user_id"].as_str().unwrap_or("N/A"));
                    info!(
                        "  Device ID: {}",
                        data["device_id"].as_str().unwrap_or("N/A")
                    );
                    info!(
                        "  Vault Name: {}",
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
    }
}
