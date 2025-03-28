use crate::node::common::model::user::common::{UserData, UserDataMember};
use crate::node::db::actions::sign_up::action::SignUpAction;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use log::info;

use serde_json::json;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::vault::vault::{VaultName, VaultStatus};
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::db::actions::sign_up::claim::SignUpClaim;
use crate::node::db::in_mem_db::InMemKvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use std::sync::Arc;

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

    let result = unsafe {
        let user_name_str = match CStr::from_ptr(user_name).to_str() {
            Ok(s) => s,
            Err(_) => {
                let error_json = json!({
                    "success": false,
                    "error": "Invalid UTF-8 in user_name"
                }).to_string();
                return CString::new(error_json).unwrap_or_default().into_raw();
            }
        };

        if user_name_str.trim().is_empty() {
            let error_json = json!({
                "success": false,
                "error": "Empty user name provided"
            }).to_string();
            return CString::new(error_json).unwrap_or_default().into_raw();
        }

        // MARK: - DATA PREPARATION
        let device_name = DeviceName::from(user_name_str);
        let vault_name = VaultName::generate();
        let user_creds = UserCredentials::generate(device_name, vault_name);
        let user_data = user_creds.user();
        let user_data_member = UserDataMember { user_data: user_data.clone() };
        
        // MARK: - FIRST ACTION
        let sign_up_action = SignUpAction;
        let events = sign_up_action.accept(user_data_member);
        let events_count = events.len();
        
        // 2. Затем сохраняем события в репозитории (асинхронно)
        // Создаем in-memory репозиторий и объект PersistentObject
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo));
        
        // Создаем SignUpClaim и вызываем метод sign_up
        let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };
        
        // Вызываем асинхронный метод sign_up через wrapper
        let vault_status_result = sync_wrapper(sign_up_claim.sign_up(user_data.clone()));
        
        // Проверяем результат
        let vault_status = match vault_status_result {
            Ok(status) => status,
            Err(err) => {
                let error_json = json!({
                    "success": false,
                    "error": format!("Failed to save sign up events: {}", err)
                }).to_string();
                return CString::new(error_json).unwrap_or_default().into_raw();
            }
        };
        
        // MARK: - JSON CREATING
        let status_type = match &vault_status {
            VaultStatus::NotExists(_) => "NotExists",
            VaultStatus::Outsider(_) => "Outsider",
            VaultStatus::Member(_) => "Member",
        };
        
        let result_json = json!({
            "success": true,
            "vault_name": user_creds.vault_name.to_string(),
            "device_name": user_creds.device().device_name.as_str(),
            "device_id": user_creds.device().device_id.to_string(),
            "events_count": events_count,
            "vault_status": status_type,
            "secret_box": serde_json::to_string(&user_creds.device_creds.secret_box).unwrap_or_default()
        });
        
        CString::new(result_json.to_string()).unwrap_or_default().into_raw()
    };

    result
}

#[no_mangle]
pub extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}