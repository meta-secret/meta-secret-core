use std::ffi::CString;
use std::future::Future;
use std::os::raw::c_char;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;
use mobile_common::mobile_manager::MobileApplicationManager;

// MARK: Helpers
static APP_MANAGER: Lazy<Mutex<Option<Arc<MobileApplicationManager>>>> =
    Lazy::new(|| Mutex::new(None));

fn sync_wrapper<F: Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all() // включаем все возможности
        .build()
        .unwrap();
    
    runtime.block_on(future)
}

fn json_to_c_string(json: &str) -> *mut c_char {
    CString::new(json).unwrap().into_raw()
}

fn c_str_to_string(ptr: *const c_char) -> String {
    if !ptr.is_null() {
        unsafe {
            std::ffi::CStr::from_ptr(ptr)
                .to_string_lossy()
                .to_string()
        }
    } else {
        String::from("")
    }
}

// MARK: Generating / init