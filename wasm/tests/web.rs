#![cfg(target_arch = "wasm32")]
///
/// https://rustwasm.github.io/wasm-bindgen/wasm-bindgen-test/index.html
///

use wasm_bindgen_test::*;
use meta_secret_web_cli::wasm_repo::{WasmMetaLogger, WasmRepo};
use meta_secret_web_cli::{configure, JsAppState};
use std::rc::Rc;
use wasm_bindgen::JsValue;
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::generic_db::FindOneQuery;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_web_cli::wasm_app_state_manager::WasmApplicationStateManager;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn pass_async() {
    configure();

    let app_manager = WasmApplicationStateManager::init_in_mem().await;
}