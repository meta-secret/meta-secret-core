#![cfg(target_arch = "wasm32")]
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::repo::generic_db::{FindOneQuery, SaveCommand};
use meta_secret_web_cli::wasm_app_state_manager::WasmApplicationStateManager;
use meta_secret_web_cli::wasm_repo::WasmRepo;
use meta_secret_web_cli::{alert, configure, info};
use std::rc::Rc;
use wasm_bindgen::JsValue;
///
/// https://rustwasm.github.io/wasm-bindgen/wasm-bindgen-test/index.html
///
use wasm_bindgen_test::*;

use indexed_db_futures::prelude::*;
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::db::events::global_index_event::{
    GlobalIndexObject, GlobalIndexRecord,
};
use meta_secret_core::node::db::events::kv_log_event::KvLogEvent;
use meta_secret_core::node::db::events::vault_event::VaultObject;
use std::time::Duration;
use tracing::info;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn pass_async() {
    WasmRepo::default().delete_db().await;
    WasmRepo::server().delete_db().await;
    WasmRepo::virtual_device().delete_db().await;

    //let obj_id = &ObjectId::global_index_unit();
    //let event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::unit());

    //repo.save(&obj_id, &event).await;
    //let db_event = repo.find_one(obj_id).await;
    //println!("{:?}", db_event);
    //alert("qwe");

    async_std::task::sleep(Duration::from_secs(1)).await;
    run_app().await;
}

async fn run_app() {
    let app_manager = WasmApplicationStateManager::init_in_mem().await;
    async_std::task::sleep(Duration::from_secs(5)).await;

    info!("Initial sign up");
    let _ = app_manager.sign_up("q", "web").await;
    async_std::task::sleep(Duration::from_secs(3)).await;
    //join
    info!("Initiate Join!");
    let _ = app_manager.sign_up("q", "web").await;

    async_std::task::sleep(Duration::from_secs(3)).await;

    info!("Cluster Distribution");
    app_manager
        .cluster_distribution("pass_id:123", "t0p$ecret")
        .await;

    async_std::task::sleep(Duration::from_secs(3)).await;
}
