#![cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
///
/// https://rustwasm.github.io/wasm-bindgen/wasm-bindgen-test/index.html
///
use wasm_bindgen_test::*;
use std::time::Duration;
use tracing::info;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_wasm::wasm_app_manager::WasmApplicationManager;
use meta_secret_wasm::wasm_repo::WasmRepo;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

//#[wasm_bindgen_test]
async fn pass_async() {
    WasmRepo::default().await;
    WasmRepo::server().await;
    WasmRepo::virtual_device().await;

    async_std::task::sleep(Duration::from_secs(1)).await;
    run_app().await;
}

async fn run_app() {
    let app_manager = WasmApplicationManager::init_wasm().await;
    async_std::task::sleep(Duration::from_secs(5)).await;

    info!("Initial sign up");
    app_manager.sign_up(VaultName::test().0).await;
    async_std::task::sleep(Duration::from_secs(3)).await;
    //join
    info!("Initiate Join!");
    app_manager.sign_up(VaultName::test().0).await;

    async_std::task::sleep(Duration::from_secs(3)).await;

    info!("Cluster Distribution");
    app_manager
        .cluster_distribution("pass_id:123", "t0p$ecret")
        .await;

    async_std::task::sleep(Duration::from_secs(3)).await;
}
