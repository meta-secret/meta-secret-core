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

