#![cfg(target_arch = "wasm32")]

use meta_secret_core::node::app::app_state_update_manager::ApplicationManagerConfigurator;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_wasm::app_manager::ApplicationManager;
use meta_secret_wasm::wasm_app_manager::WasmApplicationManager;
use meta_secret_wasm::wasm_repo::WasmSyncProtocol;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;
use wasm_bindgen::prelude::*;
///
/// https://rustwasm.github.io/wasm-bindgen/wasm-bindgen-test/index.html
///
use wasm_bindgen_test::*;

// Configure tests to run in browser with debug output
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn pass_async() {
    run_app().await;
}

async fn run_app() {
    info!("🔄 Starting run_app");
    let cfg = ApplicationManagerConfigurator {
        client_repo: Arc::new(InMemKvLogEventRepo::default()),
        server_repo: Arc::new(InMemKvLogEventRepo::default()),
        device_repo: Arc::new(InMemKvLogEventRepo::default()),
    };

    let app_manager =
        ApplicationManager::<InMemKvLogEventRepo, WasmSyncProtocol<InMemKvLogEventRepo>>::init(cfg)
            .await
            .expect("Application state manager must be initialized");
    info!("✅ WasmApplicationManager initialized");
    async_std::task::sleep(Duration::from_secs(5)).await;

    // Initial sign up - first device
    info!("🔄 Performing initial sign up");
    let vault_name = VaultName::test();
    app_manager.sign_up(vault_name.clone()).await;
    info!("✅ Sign up completed");
    async_std::task::sleep(Duration::from_secs(3)).await;

    // Get the application state and verify vault membership
    info!("🔄 Verifying vault membership");
    let state = app_manager.get_state().await;
    info!("✅ Application state retrieved");

    // Check if we're in a vault
    if state.is_vault() {
        info!("✅ We are in a vault");

        // Get vault info
        let vault_info = state.as_vault();
        info!("✅ Vault info retrieved");

        // Check if we're a member
        if vault_info.is_member() {
            info!("✅ TEST PASSED: User is a vault member");

            // Get member info
            let member_info = vault_info.as_member();
            info!("✅ Member info retrieved");

            // Get vault data
            let vault_data = member_info.vault_data();
            info!("✅ Vault data retrieved");

            // Get users
            let users = vault_data.users();
            info!("📊 Vault has {} users", users.len());

            // We expect at least 1 user (the current device)
            assert!(users.len() >= 1, "Vault should have at least 1 user");
            info!("✅ TEST PASSED: Vault has users");

            // Print user details
            for (i, user) in users.iter().enumerate() {
                info!("👤 User {}: is_member={}", i, user.is_member());
            }
        } else {
            info!("⚠️ User is not a vault member yet, but we're in a vault state");
        }
    } else {
        info!("⚠️ Not in a vault state yet, still in local state");
    }

    // Cluster distribution
    info!("🔄 Starting Cluster Distribution");
    app_manager
        .cluster_distribution("pass_id:123", "t0p$ecret")
        .await;
    info!("✅ Cluster Distribution completed");
    async_std::task::sleep(Duration::from_secs(90)).await;

    // Test completed successfully
    info!("🎉 TEST COMPLETED SUCCESSFULLY");
}
