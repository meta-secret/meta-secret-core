use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use serde_json::json;
use std::sync::Arc;
use anyhow::{anyhow};
use tracing::info;
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::common::model::device::device_creds::DeviceCredentials;
use meta_secret_core::node::common::model::vault::vault::{VaultName, VaultStatus};
use meta_secret_core::node::db::actions::sign_up::claim::SignUpClaim;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::objects::persistent_vault::PersistentVault;
use meta_secret_core::node::common::model::user::user_creds::UserCredentials;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;

fn sync_wrapper<F: std::future::Future>(future: F) -> F::Output {
    async_std::task::block_on(future)
}

#[unsafe(no_mangle)]
pub async extern "C" fn sign_up(user_name: *const c_char) -> *mut c_char {
    if user_name.is_null() {
        let error_json = json!({
            "success": false,
            "error": "Null user_name pointer provided"
        }).to_string();
        return CString::new(error_json).unwrap_or_default().into_raw();
    }

    // let result: anyhow::Result<String> = sync_wrapper(async {
        let user_name_str = unsafe { CStr::from_ptr(user_name) }
            .to_str()
            .map_err(|e| anyhow!("Invalid UTF-8 string: {}", e)).unwrap();
        let device_name = DeviceName::from(user_name_str);
        let vault_name = VaultName::from(user_name_str);
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));
        
        // let device_creds = DeviceCredentials::generate(device_name.clone());
        // let user_creds = UserCredentials::from(device_creds.clone(), vault_name.clone());
        
        let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
        let user_creds = p_creds
            .get_or_generate_user_creds(device_name.clone(), vault_name.clone())
            .await.unwrap();
        
        let sync_protocol = HttpSyncProtocol {
            api_url: ApiUrl::dev(3000),
        };
    
        let client_gw = Arc::new(SyncGateway {
            id: "mobile_client".to_string(),
            p_obj: p_obj.clone(),
            sync: Arc::new(sync_protocol),
        });
        
        client_gw.sync().await.unwrap();
        client_gw.sync().await.unwrap();
        
        let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };
        sign_up_claim.sign_up(user_creds.user().clone()).await.unwrap();

        client_gw.sync().await.unwrap();
        client_gw.sync().await.unwrap();
        
        let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
        let vault_status = p_vault.find(user_creds.user().clone()).await.unwrap();

        let result: Result<String, String> = match vault_status {
            VaultStatus::Member(member) => {
                info!("Пользователь является членом хранилища");
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
                info!("Неожиданный статус хранилища: {:?}", other_status);
                Ok(json!({
                    "success": true,
                    "status": format!("{:?}", other_status)
                }).to_string())
            }
        };
    
    
    match result {
        Ok(json_str) => CString::new(json_str).unwrap_or_default().into_raw(),
        Err(err) => {
            let error_json = json!({
                "success": false,
                "error": err.to_string()
            }).to_string();
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
    use std::time::Duration;
    use super::*;
    use serde_json::Value;
    use meta_secret_core::node::app::sync::api_url::ApiUrl;

    #[tokio::test]
    async fn test_sign_up_debug() {
        // Инициализируем логгер для тестов, чтобы видеть вывод tracing
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .finish();
        let _guard = tracing::subscriber::set_global_default(subscriber).unwrap();
        
        use reqwest::Client;
        let client = Client::new();
        let url = ApiUrl::dev(3000).get_url() + "/meta_request";

        let request_builder = client
            .get(url.as_str())
            .timeout(Duration::from_secs(3))
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", url.as_str());

        // Выводим информацию о запросе
        info!("Request: {:?}", request_builder);

        // Отправляем запрос
        let response = request_builder.send().await;
        info!("Response: {:?}", response);
        // Создаем тестовое имя пользователя
        let username = "test_debug_user_1";
        let c_username = CString::new(username).unwrap();

        info!("Вызываем sign_up для пользователя '{}'", username);
        // Вызываем функцию sign_up
        let result_ptr = sign_up(c_username.as_ptr()).await;

        // Преобразуем результат обратно в Rust строку
        let result_str = unsafe {
            assert!(!result_ptr.is_null(), "Результат sign_up - нулевой указатель");
            let result_cstr = CStr::from_ptr(result_ptr);
            let result_string = result_cstr.to_str().expect("Невалидный UTF-8").to_owned();
            
            // Освобождаем память
            free_string(result_ptr);
            
            result_string
        };
        
        info!("Получен результат: {}", result_str);
        
        // Разбираем JSON
        let json_value: Value = serde_json::from_str(&result_str).expect("Невалидный JSON");
        
        // Проверяем успешность
        let success = json_value["success"].as_bool().expect("Отсутствует поле 'success'");
        info!("Успех: {}", success);
        
        if success {
            // Проверяем статус
            if let Some(status) = json_value["status"].as_str() {
                info!("Статус: {}", status);
                
                // Если статус Member, проверяем данные
                if status == "Member" {
                    let data = &json_value["data"];
                    info!("Данные пользователя:");
                    info!("  User ID: {}", data["user_id"].as_str().unwrap_or("N/A"));
                    info!("  Device ID: {}", data["device_id"].as_str().unwrap_or("N/A"));
                    info!("  Vault Name: {}", data["vault_name"].as_str().unwrap_or("N/A"));
                } else {
                    info!("Пользователь не является членом хранилища. Статус: {}", status);
                }
            }
        } else {
            if let Some(error) = json_value["error"].as_str() {
                info!("Ошибка: {}", error);
            }
        }
    }
}