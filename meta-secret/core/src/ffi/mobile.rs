use crate::node::common::model::user::common::UserDataMember;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use serde_json::json;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::in_mem_db::InMemKvLogEventRepo;
use crate::node::common::model::user::common::UserData;
use crate::node::db::actions::sign_up::claim::SignUpClaim;
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

}

#[no_mangle]
pub extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}