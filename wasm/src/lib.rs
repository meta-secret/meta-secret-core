extern crate core;

use core::task;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use meta_secret_core::models::{ApplicationState};
use crate::wasm_app::WasmMetaClient;
use meta_secret_core::node::app::meta_app::MetaVaultManager;
use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::objects::ToJsValue;
use wasm_bindgen_futures::spawn_local;

use async_mutex::Mutex as AsyncMutex;

mod commit_log;
mod db;
mod objects;
mod utils;
mod wasm_app;
mod server;

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

#[wasm_bindgen]
pub async fn get_meta_vault() -> Result<Option<JsValue>, JsValue> {
    objects::get_meta_vault().await
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
pub async fn cluster_distribution(pass_id: &str, pass: &str) -> Result<JsValue, JsValue> {
    wasm_app::cluster_distribution(pass_id, pass).await
}

#[wasm_bindgen]
pub async fn membership(
    candidate_user_sig: JsValue,
    request_type: JsValue,
) -> Result<JsValue, JsValue> {
    wasm_app::membership(candidate_user_sig, request_type).await
}


#[wasm_bindgen]
pub async fn get_meta_passwords() -> Result<JsValue, JsValue> {
    wasm_app::get_meta_passwords().await
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

#[wasm_bindgen]
pub struct ApplicationStateManager {
    meta_client: WasmMetaClient,
    app_state: Arc<AsyncMutex<ApplicationState>>,
    js_app_state: JsAppState
}

#[wasm_bindgen]
impl ApplicationStateManager {

    pub fn new(js_app_state: JsAppState) -> ApplicationStateManager {
        let app_state = {
            let state = ApplicationState {
                meta_vault: None,
                join_component: false,
            };
            Arc::new(AsyncMutex::new(state))
        };

        let meta_client = WasmMetaClient::new(app_state.clone());
        ApplicationStateManager {
            app_state,
            meta_client,
            js_app_state
        }
    }

    pub async fn init(&self) {
        let logger = WasmMetaLogger {};

        let repo = WasmRepo::default();
        let meta_vault = repo
            .find_meta_vault(&logger)
            .await
            .unwrap();

        {
            let mut app_state = self.app_state.lock().await;
            app_state.meta_vault = meta_vault.map(Box::new);
            self.on_update();
        }
    }

    pub fn on_update(&self) {
        // update app state in vue
        self.js_app_state.updateJsState();
    }

    pub async fn get_state(&self) -> JsValue {
        let app_state = self.app_state.lock().await;
        app_state.to_js().unwrap()
    }

    pub async fn sign_up(&self, vault_name: &str, device_name: &str) {
        self.meta_client.sign_up(vault_name, device_name).await;
        self.on_update();
    }
}