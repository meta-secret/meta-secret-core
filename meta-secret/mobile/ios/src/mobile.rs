use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::future::Future;

use serde_json::json;
use std::sync::Arc;
use anyhow::{anyhow};
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
    // Создаем новый runtime для выполнения асинхронного кода
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all() // включаем все возможности
        .build()
        .unwrap();
        
    // Блокирующий вызов future в runtime
    runtime.block_on(future)
}

/// Регистрация нового пользователя
/// 
/// @param user_name Имя пользователя в UTF-8 строке
/// @return JSON строка с результатом операции; нужно освободить память с помощью free_string
#[unsafe(no_mangle)]
pub extern "C" fn sign_up(user_name: *const c_char) -> *mut c_char {
    // Инициализация системы логирования
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("Вызов функции sign_up");
    
    if user_name.is_null() {
        let error_json = json!({
            "success": false,
            "error": "Null user_name pointer provided"
        }).to_string();
        return CString::new(error_json).unwrap_or_default().into_raw();
    }

    let result: anyhow::Result<String> = sync_wrapper(async {
        info!("Начало асинхронной операции sign_up");
        
        let user_name_str = unsafe { CStr::from_ptr(user_name) }
            .to_str()
            .map_err(|e| anyhow!("Invalid UTF-8 string: {}", e))?;
            
        info!("Полученное имя пользователя: {}", user_name_str);
        
        let device_name = DeviceName::from(user_name_str);
        let vault_name = VaultName::from(user_name_str);
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));

        info!("Генерирование учетных данных пользователя");
        let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
        let user_creds = p_creds
            .get_or_generate_user_creds(device_name.clone(), vault_name.clone())
            .await?;

        info!("Настройка HTTP протокола синхронизации");
        let sync_protocol = HttpSyncProtocol {
            api_url: ApiUrl::custom_dev("http://192.168.0.112", 3000),
        };

        info!("Создание шлюза синхронизации");
        let client_gw = Arc::new(SyncGateway {
            id: "mobile_client".to_string(),
            p_obj: p_obj.clone(),
            sync: Arc::new(sync_protocol),
        });

        info!("Первая синхронизация");
        client_gw.sync().await?;
        
        info!("Вторая синхронизация");
        client_gw.sync().await?;

        info!("Выполнение операции sign_up");
        let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };
        sign_up_claim.sign_up(user_creds.user().clone()).await?;

        info!("Третья синхронизация");
        client_gw.sync().await?;
        
        info!("Четвёртая синхронизация");
        client_gw.sync().await?;

        info!("Проверка статуса хранилища");
        let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
        let vault_status = p_vault.find(user_creds.user().clone()).await?;

        info!("Статус: {:?}", vault_status);
        
        match vault_status {
            VaultStatus::Member(member) => {
                info!("Пользователь является членом хранилища!");
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
                info!("Недопустимый статус хранилища: {:?}", other_status);
                Ok(json!({
                    "success": true,
                    "status": format!("{:?}", other_status)
                }).to_string())
            }
        }
    });

    info!("Асинхронная операция завершена, результат: {:?}", result.is_ok());

    match result {
        Ok(json_str) => CString::new(json_str).unwrap_or_default().into_raw(),
        Err(err) => {
            let error_msg = err.to_string();
            info!("Ошибка: {}", error_msg);
            
            let error_json = json!({
                "success": false,
                "error": error_msg
            }).to_string();
            CString::new(error_json).unwrap_or_default().into_raw()
        }
    }
}

/// Освобождает память, выделенную для строки
/// 
/// @param ptr Указатель на строку, возвращенную любой функцией API
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

    #[test]
    fn test_sign_up_debug() {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .finish();
        let _guard = tracing::subscriber::set_global_default(subscriber).unwrap();
        
        // Проверяем доступность сервера перед вызовом sign_up
        info!("Проверка доступности сервера...");
        let result = sync_wrapper(async {
            reqwest::Client::new()
                .get(format!("{}/meta_request", ApiUrl::custom_dev("http://192.168.0.112", 3000).get_url()))
                .timeout(Duration::from_secs(3))
                .send()
                .await
        });
            
        info!("Результат проверки доступности сервера: {:?}", result);
        
        if result.is_err() {
            info!("ВНИМАНИЕ: Сервер не доступен. Тест может зависнуть!");
        }
        
        // Создаем имя пользователя и запускаем sign_up
        let username = "test_debug_user_1";
        let c_username = CString::new(username).unwrap();

        info!("Запуск sign_up для пользователя: '{}'", username);
        let result_ptr = sign_up(c_username.as_ptr());
        
        // Проверяем результат
        let result_str = unsafe {
            assert!(!result_ptr.is_null(), "sign_up - null pointer");
            let result_cstr = CStr::from_ptr(result_ptr);
            let result_string = result_cstr.to_str().expect("Invalid UTF-8").to_owned();
            
            free_string(result_ptr);
            
            result_string
        };
        
        info!("Получен результат: {}", result_str);
        
        // Парсим JSON результат
        let json_value: Value = serde_json::from_str(&result_str).expect("Invalid JSON");
        
        // Проверяем успешность
        let success = json_value["success"].as_bool().expect("missing filed: 'success'");
        info!("Успех: {}", success);
        
        if success {
            // Проверяем статус
            if let Some(status) = json_value["status"].as_str() {
                info!("Статус: {}", status);
                
                // Если статус Member, проверяем данные
                if status == "Member" {
                    let data = &json_value["data"];
                    info!("Информация о пользователе:");
                    info!("  ID пользователя: {}", data["user_id"].as_str().unwrap_or("N/A"));
                    info!("  ID устройства: {}", data["device_id"].as_str().unwrap_or("N/A"));
                    info!("  Имя хранилища: {}", data["vault_name"].as_str().unwrap_or("N/A"));
                } else {
                    info!("Пользователь не является членом. Статус: {}", status);
                }
            }
        } else {
            if let Some(error) = json_value["error"].as_str() {
                info!("Ошибка: {}", error);
            }
        }
    }
}