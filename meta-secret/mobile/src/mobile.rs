use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use serde_json::json;
use std::sync::Arc;
use anyhow::{anyhow};
use tracing::info;
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

fn sync_wrapper<F: std::future::Future>(future: F) -> F::Output {
    async_std::task::block_on(future)
}

#[unsafe(no_mangle)]
pub extern "C" fn sign_up(user_name: *const c_char) -> *mut c_char {
    if user_name.is_null() {
        let error_json = json!({
            "success": false,
            "error": "Null user_name pointer provided"
        }).to_string();
        return CString::new(error_json).unwrap_or_default().into_raw();
    }

    let result: anyhow::Result<String> = sync_wrapper(async {
        let user_name_str = unsafe { CStr::from_ptr(user_name) }
            .to_str()
            .map_err(|e| anyhow!("Invalid UTF-8 string: {}", e))?;

        let device_name = DeviceName::from(user_name_str);
        let device_creds = DeviceCredentials::generate(device_name);
        let vault_name = VaultName::from(user_name_str);
        let user_creds = UserCredentials::from(device_creds, vault_name);
        
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));
        
        let sync_protocol = HttpSyncProtocol{};
        let client_gw = Arc::new(SyncGateway {
            id: "mobile_client".to_string(),
            p_obj: p_obj.clone(),
            sync: Arc::new(sync_protocol),
        });
        
        client_gw.sync().await.expect("Wrong sync 1");
        client_gw.sync().await.expect("Wrong sync 2");
        
        let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };
        sign_up_claim.sign_up(user_creds.user().clone()).await.expect("SignUpClaim failed");
        
        client_gw.sync().await.expect("Wrong sync 3");
        client_gw.sync().await.expect("Wrong sync 4");
        
        let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
        let vault_status = p_vault
            .find(user_creds.user().clone())
            .await?;

        match vault_status {
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
        }
    });
    
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
    use super::*;
    use serde_json::Value;
    
    #[tokio::test]
    async fn test_sign_up_debug() {
        // Инициализируем логгер для тестов, чтобы видеть вывод tracing
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        
        // Создаем тестовое имя пользователя
        let username = "test_debug_user";
        let c_username = CString::new(username).unwrap();
        
        println!("Вызываем sign_up для пользователя '{}'", username);
        // Вызываем функцию sign_up
        let result_ptr = sign_up(c_username.as_ptr());
        
        // Преобразуем результат обратно в Rust строку
        let result_str = unsafe {
            assert!(!result_ptr.is_null(), "Результат sign_up - нулевой указатель");
            let result_cstr = CStr::from_ptr(result_ptr);
            let result_string = result_cstr.to_str().expect("Невалидный UTF-8").to_owned();
            
            // Освобождаем память
            free_string(result_ptr);
            
            result_string
        };
        
        println!("Получен результат: {}", result_str);
        
        // Разбираем JSON
        let json_value: Value = serde_json::from_str(&result_str).expect("Невалидный JSON");
        
        // Проверяем успешность
        let success = json_value["success"].as_bool().expect("Отсутствует поле 'success'");
        println!("Успех: {}", success);
        
        if success {
            // Проверяем статус
            let status = json_value["status"].as_str().expect("Отсутствует поле 'status'");
            println!("Статус: {}", status);
            
            // Если статус Member, проверяем данные
            if status == "Member" {
                let data = &json_value["data"];
                println!("Данные пользователя:");
                println!("  User ID: {}", data["user_id"].as_str().unwrap_or("N/A"));
                println!("  Device ID: {}", data["device_id"].as_str().unwrap_or("N/A"));
                println!("  Vault Name: {}", data["vault_name"].as_str().unwrap_or("N/A"));
            } else {
                println!("Пользователь не является членом хранилища. Статус: {}", status);
            }
        } else {
            let error = json_value["error"].as_str().expect("Отсутствует поле 'error'");
            println!("Ошибка: {}", error);
        }
    }
}