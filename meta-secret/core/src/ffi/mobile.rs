use std::ffi::{c_char, CStr};
use std::os::raw::c_int;
use std::sync::Arc;

use crate::node::common::model::user::common::{UserData, UserDataOutsiderStatus, UserMembership};
use crate::node::common::model::vault::vault::VaultMember;
use crate::node::db::events::vault::vault_log_event::JoinClusterEvent;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::actions::sign_up::join::AcceptJoinAction;

// Константы для статусов Join
pub const JOIN_STATUS_SUCCESS: c_int = 0;
pub const JOIN_STATUS_PENDING: c_int = 1;
pub const JOIN_STATUS_DECLINED: c_int = 2;
pub const JOIN_STATUS_ALREADY_MEMBER: c_int = 3;
pub const JOIN_STATUS_ERROR: c_int = -1;

// Глобальный контекст хранилища для мобильного приложения
static mut MOBILE_CONTEXT: Option<MobileContext> = None;

struct MobileContext<Repo: KvLogEventRepo> {
    p_obj: Arc<PersistentObject<Repo>>,
    current_member: VaultMember,
}

#[no_mangle]
pub unsafe extern "C" fn meta_secret_init(db_path: *const c_char) -> c_int {
    let c_str = CStr::from_ptr(db_path);
    let db_path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return JOIN_STATUS_ERROR,
    };

    // Инициализация хранилища по указанному пути
    // В реальном коде здесь должно быть создание PersistentObject
    // и инициализация VaultMember на основе текущего пользователя
    match initialize_storage(db_path_str) {
        Ok((p_obj, current_member)) => {
            MOBILE_CONTEXT = Some(MobileContext { p_obj, current_member });
            JOIN_STATUS_SUCCESS
        },
        Err(_) => JOIN_STATUS_ERROR,
    }
}

#[no_mangle]
pub unsafe extern "C" fn meta_secret_join_cluster(candidate_id: *const c_char) -> c_int {
    // Проверка инициализации контекста
    let context = match &MOBILE_CONTEXT {
        Some(ctx) => ctx,
        None => return JOIN_STATUS_ERROR,
    };

    let c_str = CStr::from_ptr(candidate_id);
    let candidate_id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return JOIN_STATUS_ERROR,
    };

    // Создаем запрос на присоединение на основе ID кандидата
    // (В реальном коде нужно преобразовать ID в UserData)
    let candidate = match create_user_data_from_id(candidate_id_str) {
        Ok(user) => user,
        Err(_) => return JOIN_STATUS_ERROR,
    };

    let join_request = JoinClusterEvent { candidate };

    // Создаем AcceptJoinAction и вызываем метод accept
    let action = AcceptJoinAction {
        p_obj: context.p_obj.clone(),
        member: context.current_member.clone(),
    };

    // Вызываем метод accept и обрабатываем результат
    match tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(action.accept(join_request)) {
        Ok(_) => JOIN_STATUS_SUCCESS,
        Err(err) => {
            let err_msg = err.to_string();
            if err_msg.contains("User already in pending state") {
                JOIN_STATUS_PENDING
            } else if err_msg.contains("User request already declined") {
                JOIN_STATUS_DECLINED
            } else if err_msg.contains("Membership cannot be accepted") {
                JOIN_STATUS_ALREADY_MEMBER
            } else {
                JOIN_STATUS_ERROR
            }
        }
    }
}

// Вспомогательные функции (заглушки для примера)
fn initialize_storage<Repo: KvLogEventRepo>(db_path: &str) -> Result<(Arc<PersistentObject<Repo>>, VaultMember), String> {
    // Здесь должна быть реальная инициализация хранилища
    // и получение текущего пользователя
    Err("Not implemented".to_string())
}

fn create_user_data_from_id(id: &str) -> Result<UserData, String> {
    // Здесь должно быть создание UserData на основе ID пользователя
    Err("Not implemented".to_string())
}