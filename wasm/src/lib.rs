extern crate core;

use wasm_bindgen::prelude::*;

mod commit_log;
mod db;
mod objects;
mod utils;
pub mod wasm_app;
mod virtual_device;
mod wasm_sync_gateway;
mod app_state_manager;
mod wasm_server;

/// Json utilities https://github.com/rustwasm/wasm-bindgen/blob/main/crates/js-sys/tests/wasm/JSON.rs

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    pub type JsAppState;

    #[wasm_bindgen(structural, method)]
    pub fn updateJsState(this: &JsAppState);
}

#[wasm_bindgen]
extern "C" {
    pub fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
    pub async fn idbGet(db_name: &str, store_name: &str, key: &str) -> JsValue;
    pub async fn idbSave(db_name: &str, store_name: &str, key: &str, value: JsValue);

    pub async fn idbFindAll(db_name: &str, store_name: &str) -> JsValue;
}

#[wasm_bindgen]
pub fn configure() {
    utils::set_panic_hook();
}

///https://rustwasm.github.io/wasm-bindgen/examples/closures.html
#[wasm_bindgen]
pub async fn recover() -> Result<JsValue, JsValue> {
    log("wasm recover!");

    /*
    server_api::claim_for_password_recovery(&recovery_request)
    */

    Ok(JsValue::null())
}

/// Sync local commit log with server
#[wasm_bindgen]
pub async fn sync() -> Result<JsValue, JsValue> {
    wasm_app::sync_shares().await
}

#[wasm_bindgen]
pub async fn membership(
    candidate_user_sig: JsValue,
    request_type: JsValue,
) -> Result<JsValue, JsValue> {
    wasm_app::membership(candidate_user_sig, request_type).await
}


/// https://rustwasm.github.io/docs/wasm-bindgen/reference/arbitrary-data-with-serde.html
#[wasm_bindgen]
pub fn split(pass: &str) -> Result<JsValue, JsValue> {
    wasm_app::split(pass)
}

#[wasm_bindgen]
pub fn restore_password(shares_json: JsValue) -> Result<JsValue, JsValue> {
    /*log("wasm: restore password, core functionality");

    let user_shares: Vec<UserShareDto> = serde_wasm_bindgen::from_value(shares_json)?;

    let plain_text = recover_from_shares(user_shares).map_err(JsError::from)?;
    Ok(JsValue::from_str(plain_text.text.as_str()))*/
    Ok(JsValue::null())
}
