#![cfg(target_arch = "wasm32")]
///
/// https://rustwasm.github.io/wasm-bindgen/wasm-bindgen-test/index.html
///

use wasm_bindgen_test::*;
use meta_secret_web_cli::wasm_repo::{WasmMetaLogger, WasmRepo};
use meta_secret_web_cli::configure;
use std::rc::Rc;
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::generic_db::FindOneQuery;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn pass_async() {
    configure();

    let client_repo = Rc::new(InMemKvLogEventRepo::default());
    let maybe_obj = client_repo.find_one(&ObjectId::vault_unit("test-vault")).await.unwrap();
    assert!(maybe_obj.is_none());
}