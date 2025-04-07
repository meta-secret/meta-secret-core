use crate::node::common::model::user::common::UserDataMember;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use serde_json::json;
use crate::node::common::model::device::common::{DeviceData, DeviceName};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::in_mem_db::InMemKvLogEventRepo;
use crate::node::common::model::user::common::UserData;
use crate::node::db::actions::sign_up::claim::SignUpClaim;
use std::sync::Arc;
use anyhow::{Result, anyhow};
use crate::node::app::sync::sync_gateway::SyncGateway;
use crate::node::common::model::device::device_creds::DeviceCredentials;
use meta_server_node::server::server_sync_protocol::fixture::EmbeddedSyncProtocol;
use meta_server_node::server::server_app::ServerApp;
use crate::node::common::model::vault::vault::VaultStatus;
use crate::node::db::repo::persistent_credentials::spec::PersistentCredentialsSpec;
use tracing::info;
use tracing_subscriber::util::SubscriberInitExt;
use crate::node::db::descriptors::creds::CredentialsDescriptor::User;

fn sync_wrapper<F: std::future::Future>(future: F) -> F::Output {
    async_std::task::block_on(future)
}

#[no_mangle]
pub extern "C" fn sign_up(user_name: *const c_char) -> *mut c_char {
    if user_name.is_null() {
        let error_json = json!({
            "success": false,
            "error": "Null user_name pointer provided"
        }).to_string();
        return CString::new(error_json).unwrap_or_default().into_raw();
    }

    let result = sync_wrapper(async {
        // Получаем имя пользователя из C строки
        let user_name_str = unsafe { CStr::from_ptr(user_name) }
            .to_str()
            .map_err(|e| anyhow!("Invalid UTF-8 string: {}", e))?;
        
        // Создаем девайс 
        let device_name = DeviceName::from(user_name_str);
        let vault_name = VaultName::from(user_name_str);
        
        // Создаем репозиторий и персистентный объект
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));
        
        // Генерируем учетные данные устройства
        let device_creds = DeviceCredentials::generate(device_name.clone());
        
        // Создаем данные пользователя
        let user_data = UserData {
            vault_name,
            device: device_creds.device.clone(),
        };
        
        // Создаем серверный репозиторий и приложение
        let server_repo = Arc::new(InMemKvLogEventRepo::default());
        let server_p_obj = Arc::new(PersistentObject::new(server_repo.clone()));
        let server_app = Arc::new(ServerApp::new(server_repo)?);
        
        // Инициализируем сервер
        server_app.init().await?;
        
        // Создаем SyncGateway для клиента
        let sync_protocol = EmbeddedSyncProtocol::new(server_repo.clone());
        let client_gw = Arc::new(SyncGateway::new(sync_protocol, p_obj.clone()));
        
        // Создаем спецификацию учетных данных сервера
        let server_creds_spec = PersistentCredentialsSpec::from(server_p_obj.clone());
        
        // Верифицируем учетные данные устройства
        server_creds_spec.verify_device_creds().await?;
        
        // Синхронизируем клиентский шлюз
        client_gw.sync().await?;
        client_gw.sync().await?;
        
        // Выполняем регистрацию (sign up)
        let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };
        sign_up_claim.sign_up(user_data.clone()).await?;
        
        // Синхронизируем шлюз еще раз
        client_gw.sync().await?;
        client_gw.sync().await?;
        
        // Проверяем статус хранилища
        let p_vault = Arc::new(crate::node::db::objects::persistent_vault::PersistentVault::from(p_obj.clone()));
        let vault_status = p_vault.find(user_data).await?;
        
        // Преобразуем статус в JSON для возврата
        match vault_status {
            VaultStatus::Member(member) => {
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
                info!("Unexpected vault status: {:?}", other_status);
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

#[no_mangle]
pub extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}