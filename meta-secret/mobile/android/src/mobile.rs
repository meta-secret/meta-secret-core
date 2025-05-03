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
/// @param env JNI окружение
/// @param _class JNI класс (не используется)
/// @param user_name Имя пользователя в JString
/// @return jstring с JSON-результатом операции
#[unsafe(no_mangle)]
pub extern "C" fn Java_sharedData_MetaSecretCoreServiceAndroid_00024NativeLib_sign_1up
(mut env: JNIEnv, _: JClass, user_name: JString) -> jstring {
    // Инициализация системы логирования
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("Вызов функции sign_up");

    // Конвертируем JString в строку Rust
    let user_name_result = match env.get_string(&user_name) {
        Ok(s) => s.to_string_lossy().to_string(),
        Err(e) => {
            let error_json = json!({
                "success": false,
                "error": format!("Ошибка преобразования JString: {}", e)
            }).to_string();
            
            match env.new_string(error_json) {
                Ok(s) => return s.into_raw(),
                Err(_) => return std::ptr::null_mut(),
            }
        }
    };
    
    info!("Полученное имя пользователя: {}", user_name_result);

    let result: anyhow::Result<String> = sync_wrapper(async {
        info!("Начало асинхронной операции sign_up");
        
        let device_name = DeviceName::from(user_name_result.as_str());
        let vault_name = VaultName::from(user_name_result.as_str());
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));

        info!("Генерирование учетных данных пользователя");
        let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
        let user_creds = p_creds
            .get_or_generate_user_creds(device_name.clone(), vault_name.clone())
            .await?;

        info!("Настройка HTTP протокола синхронизации");
        let sync_protocol = HttpSyncProtocol {
            api_url: ApiUrl::prod(),
        };

        info!("Создание шлюза синхронизации");
        let client_gw = Arc::new(SyncGateway {
            id: "mobile_client".to_string(),
            p_obj: p_obj.clone(),
            sync: Arc::new(sync_protocol),
            device_creds: Arc::new(user_creds.device_creds.clone())
        });

        info!("Первая синхронизация");
        client_gw.sync(user_creds.user()).await?;
        
        info!("Вторая синхронизация");
        client_gw.sync(user_creds.user()).await?;

        info!("Выполнение операции sign_up");
        let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };
        sign_up_claim.sign_up(user_creds.user().clone()).await?;

        info!("Третья синхронизация");
        client_gw.sync(user_creds.user()).await?;
        
        info!("Четвёртая синхронизация");
        client_gw.sync(user_creds.user()).await?;

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
        Ok(json_str) => match env.new_string(json_str) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(err) => {
            let error_msg = err.to_string();
            info!("Ошибка: {}", error_msg);
            
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

/// Освобождает память, выделенную для строки (используется в JNI-мосте)
/// 
/// @param _env JNI окружение
/// @param _class JNI класс (не используется)
/// @param _ptr Указатель на строку, возвращенную любой функцией API
#[unsafe(no_mangle)]
pub extern "C" fn Java_sharedData_MetaSecretCoreServiceAndroid_00024NativeLib_free_1string
(_env: JNIEnv, _: JClass, _ptr: jstring) {
    info!("JNI free_string called");
    // В JNI среде освобождение строк происходит автоматически через сборщик мусора Java
    // Данная функция оставлена для API-совместимости с iOS версией
}

/// Получение информации о статусе устройства, пользователя и хранилища
/// 
/// @param env JNI окружение
/// @param _class JNI класс (не используется)
/// @return jstring с JSON-результатом операции
#[unsafe(no_mangle)]
pub extern "C" fn Java_sharedData_MetaSecretCoreServiceAndroid_00024NativeLib_get_1info
(mut env: JNIEnv, _: JClass) -> jstring {
    // Инициализация системы логирования
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("Вызов функции get_info");

    let result: anyhow::Result<String> = sync_wrapper(async {
        info!("Начало асинхронной операции get_info");
        
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));

        let p_creds = PersistentCredentials {
            p_obj: p_obj.clone(),
        };

        // Проверяем, есть ли учетные данные устройства
        let maybe_device_creds = p_creds.get_device_creds().await?;

        // Если устройство не инициализировано, возвращаем соответствующий статус
        if maybe_device_creds.is_none() {
            info!("Устройство не инициализировано");
            return Ok(json!({
                "success": true,
                "status": "NotInitialized",
                "message": "Устройство не инициализировано"
            }).to_string());
        }

        let device_creds = maybe_device_creds.unwrap().value();

        // Получаем данные пользователя
        let maybe_user_creds = p_creds.get_user_creds().await?;

        // Если пользователь не инициализирован, возвращаем информацию только об устройстве
        if maybe_user_creds.is_none() {
            info!("Только устройство инициализировано, пользователь отсутствует");
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

        // Создаем клиентский шлюз
        let sync_protocol = HttpSyncProtocol {
            api_url: ApiUrl::prod(),
        };

        let client_gw = Arc::new(SyncGateway {
            id: "mobile_client".to_string(),
            p_obj: p_obj.clone(),
            sync: Arc::new(sync_protocol),
            device_creds: Arc::new(device_creds.clone()),
        });

        // Синхронизируемся для получения актуальной информации
        match client_gw.sync(user_creds.user()).await {
            Err(e) => {
                info!("Синхронизация не удалась: {}", e);
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
            _ => info!("Синхронизация выполнена успешно")
        }

        // Получаем состояние хранилища
        let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
        let vault_status = p_vault.find(user_creds.user().clone()).await?;

        info!("Статус хранилища: {:?}", vault_status);

        // Формируем результат в зависимости от статуса хранилища
        match vault_status {
            VaultStatus::Member(member) => {
                info!("Пользователь является членом хранилища");
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
                info!("Пользователь является внешним");
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
                info!("Хранилище не существует");
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
                info!("Неизвестный статус хранилища");
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

    info!("Асинхронная операция get_info завершена, результат: {:?}", result.is_ok());

    match result {
        Ok(json_str) => match env.new_string(json_str) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(err) => {
            let error_msg = err.to_string();
            info!("Ошибка: {}", error_msg);
            
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

        let username = "test_android_user_1";
        
        info!("Тест sign_up для пользователя: '{}'", username);
        
        // Чтобы тест не зависел от JVM, мы протестируем только внутреннюю асинхронную функцию
        let result: anyhow::Result<String> = sync_wrapper(async {
            let device_name = DeviceName::from(username);
            let vault_name = VaultName::from(username);
            let repo = Arc::new(InMemKvLogEventRepo::default());
            let p_obj = Arc::new(PersistentObject::new(repo.clone()));

            info!("Генерирование учетных данных пользователя");
            let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
            let user_creds = p_creds
                .get_or_generate_user_creds(device_name.clone(), vault_name.clone())
                .await?;

            info!("Настройка HTTP протокола синхронизации");
            let sync_protocol = HttpSyncProtocol {
                api_url: ApiUrl::prod(),
            };

            info!("Создание шлюза синхронизации");
            let client_gw = Arc::new(SyncGateway {
                id: "mobile_client".to_string(),
                p_obj: p_obj.clone(),
                sync: Arc::new(sync_protocol),
                device_creds: Arc::new(user_creds.device_creds.clone())
            });

            info!("Первая синхронизация");
            client_gw.sync(user_creds.user()).await?;
            
            info!("Вторая синхронизация");
            client_gw.sync(user_creds.user()).await?;

            info!("Выполнение операции sign_up");
            let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };
            sign_up_claim.sign_up(user_creds.user().clone()).await?;

            info!("Третья синхронизация");
            client_gw.sync(user_creds.user()).await?;
            
            info!("Четвёртая синхронизация");
            client_gw.sync(user_creds.user()).await?;

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

        match result {
            Ok(result_str) => {
                info!("Результат: {}", result_str);

                let json_value: Value = serde_json::from_str(&result_str).expect("Неверный JSON");

                let success = json_value["success"]
                    .as_bool()
                    .expect("отсутствует поле 'success'");
                info!("Успех: {}", success);

                if success {
                    if let Some(status) = json_value["status"].as_str() {
                        info!("Статус: {}", status);

                        if status == "Member" {
                            let data = &json_value["data"];
                            info!("Информация о пользователе:");
                            info!("  ID пользователя: {}", data["user_id"].as_str().unwrap_or("N/A"));
                            info!(
                                "  ID устройства: {}",
                                data["device_id"].as_str().unwrap_or("N/A")
                            );
                            info!(
                                "  Название хранилища: {}",
                                data["vault_name"].as_str().unwrap_or("N/A")
                            );
                        } else {
                            info!("Пользователь не является членом. Статус: {}", status);
                        }
                    }
                } else {
                    if let Some(error) = json_value["error"].as_str() {
                        info!("Ошибка: {}", error);
                    }
                }
            },
            Err(e) => {
                info!("Ошибка при выполнении: {}", e);
                panic!("Тест завершился с ошибкой: {}", e);
            }
        }
    }

    #[test]
    #[ignore]
    fn test_get_info_android() {
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

        info!("Тест get_info");
        
        // Тестируем только внутреннюю асинхронную функцию
        let result: anyhow::Result<String> = sync_wrapper(async {
            let repo = Arc::new(InMemKvLogEventRepo::default());
            let p_obj = Arc::new(PersistentObject::new(repo.clone()));

            let p_creds = PersistentCredentials {
                p_obj: p_obj.clone(),
            };

            // Проверяем, есть ли учетные данные устройства
            let maybe_device_creds = p_creds.get_device_creds().await?;

            // Если устройство не инициализировано, возвращаем соответствующий статус
            if maybe_device_creds.is_none() {
                info!("Устройство не инициализировано");
                return Ok(json!({
                    "success": true,
                    "status": "NotInitialized",
                    "message": "Устройство не инициализировано"
                }).to_string());
            }

            let device_creds = maybe_device_creds.unwrap().value();

            // Получаем данные пользователя
            let maybe_user_creds = p_creds.get_user_creds().await?;

            // Если пользователь не инициализирован, возвращаем информацию только об устройстве
            if maybe_user_creds.is_none() {
                info!("Только устройство инициализировано, пользователь отсутствует");
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

            // Создаем клиентский шлюз
            let sync_protocol = HttpSyncProtocol {
                api_url: ApiUrl::prod(),
            };

            let client_gw = Arc::new(SyncGateway {
                id: "mobile_client".to_string(),
                p_obj: p_obj.clone(),
                sync: Arc::new(sync_protocol),
                device_creds: Arc::new(device_creds.clone()),
            });

            // Синхронизируемся для получения актуальной информации
            match client_gw.sync(user_creds.user()).await {
                Err(e) => {
                    info!("Синхронизация не удалась: {}", e);
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
                _ => info!("Синхронизация выполнена успешно")
            }

            // Получаем состояние хранилища
            let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
            let vault_status = p_vault.find(user_creds.user().clone()).await?;

            info!("Статус хранилища: {:?}", vault_status);

            // Формируем результат в зависимости от статуса хранилища
            match vault_status {
                VaultStatus::Member(member) => {
                    info!("Пользователь является членом хранилища");
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
                    info!("Пользователь является внешним");
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
                    info!("Хранилище не существует");
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
                    info!("Неизвестный статус хранилища");
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
                info!("Результат: {}", result_str);

                let json_value: Value = serde_json::from_str(&result_str).expect("Неверный JSON");

                let success = json_value["success"]
                    .as_bool()
                    .expect("отсутствует поле 'success'");
                info!("Успех: {}", success);

                if success {
                    if let Some(status) = json_value["status"].as_str() {
                        info!("Статус: {}", status);

                        match status {
                            "NotInitialized" => {
                                info!("Устройство не инициализировано");
                            },
                            "DeviceOnly" => {
                                let device = &json_value["device"];
                                info!("Информация об устройстве:");
                                info!("  ID устройства: {}", device["id"].as_str().unwrap_or("N/A"));
                                info!("  Имя устройства: {}", device["name"].as_str().unwrap_or("N/A"));
                            },
                            "Member" => {
                                let device = &json_value["device"];
                                let vault = &json_value["vault"];
                                
                                info!("Информация об устройстве:");
                                info!("  ID устройства: {}", device["id"].as_str().unwrap_or("N/A"));
                                info!("  Имя устройства: {}", device["name"].as_str().unwrap_or("N/A"));
                                
                                info!("Информация о хранилище:");
                                info!("  Название хранилища: {}", vault["name"].as_str().unwrap_or("N/A"));
                                info!("  ID владельца: {}", vault["owner_id"].as_str().unwrap_or("N/A"));
                                
                                if let Some(users) = vault["users"].as_array() {
                                    info!("Пользователи в хранилище:");
                                    for user in users {
                                        let user_type = user["type"].as_str().unwrap_or("Неизвестно");
                                        let device_id = user["device_id"].as_str().unwrap_or("N/A");
                                        if user_type == "Member" {
                                            let device_name = user["device_name"].as_str().unwrap_or("N/A");
                                            info!("  Участник: {} ({})", device_name, device_id);
                                        } else {
                                            info!("  Внешний: {}", device_id);
                                        }
                                    }
                                }
                                
                                if let Some(secrets) = vault["secrets"].as_array() {
                                    info!("Секреты в хранилище:");
                                    for secret in secrets {
                                        let secret_id = secret["id"].as_str().unwrap_or("N/A");
                                        let secret_name = secret["name"].as_str().unwrap_or("N/A");
                                        info!("  Секрет: {} ({})", secret_name, secret_id);
                                    }
                                }
                            },
                            _ => {
                                info!("Другой статус: {}", status);
                            }
                        }
                    }
                } else {
                    if let Some(error) = json_value["error"].as_str() {
                        info!("Ошибка: {}", error);
                    }
                }
            },
            Err(e) => {
                info!("Ошибка при выполнении: {}", e);
                panic!("Тест завершился с ошибкой: {}", e);
            }
        }
    }
}

