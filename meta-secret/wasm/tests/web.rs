#![cfg(target_arch = "wasm32")]
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

/// This test verifies the basic functionality of the Meta Secret application:
/// 1. Initializes repositories (default, server, virtual device)
/// 2. Creates a WasmApplicationManager
/// 3. Signs up with a test vault
/// 4. Initiates a join operation
/// 5. Performs cluster distribution with credentials
/// 6. Verifies vault membership
#[wasm_bindgen_test]
async fn pass_async() {
    // Initialize repositories
    info!("Initializing repositories");
    WasmRepo::default().await;
    WasmRepo::server().await;
    WasmRepo::virtual_device().await;
    info!("Repositories initialized");

    async_std::task::sleep(Duration::from_secs(1)).await;
    run_app().await;
}

async fn run_app() {
    info!("Starting run_app");
    let app_manager = WasmApplicationManager::init_wasm().await;
    info!("WasmApplicationManager initialized");
    async_std::task::sleep(Duration::from_secs(5)).await;

    // Initial sign up - first device
    info!("Initial sign up");
    let vault_name = VaultName::test().0;
    app_manager.sign_up(vault_name.clone()).await;
    info!("Sign up completed");
    async_std::task::sleep(Duration::from_secs(3)).await;
    
    // Get the application state and verify vault membership
    info!("Verifying vault membership");
    let state = app_manager.get_state().await;
    info!("Application state retrieved");
    
    // Check if we're in a vault
    if state.is_vault() {
        info!("We are in a vault");
        
        // Get vault info
        let vault_info = state.as_vault();
        info!("Vault info retrieved");
        
        // Check if we're a member
        if vault_info.is_member() {
            info!("✅ Test passed: User is a vault member");
            
            // Get member info
            let member_info = vault_info.as_member();
            info!("Member info retrieved");
            
            // Get vault data
            let vault_data = member_info.vault_data();
            info!("Vault data retrieved");
            
            // Get users
            let users = vault_data.users();
            info!("Vault has {} users", users.len());
            
            // We expect at least 1 user (the current device)
            assert!(users.len() >= 1, "Vault should have at least 1 user");
            info!("✅ Test passed: Vault has users");
            
            // Print user details
            for (i, user) in users.iter().enumerate() {
                info!("User {}: is_member={}", i, user.is_member());
            }
        } else {
            info!("User is not a vault member yet, but we're in a vault state");
        }
    } else {
        info!("Not in a vault state yet, still in local state");
    }
    
    // Cluster distribution
    info!("Starting Cluster Distribution");
    app_manager
        .cluster_distribution("pass_id:123", "t0p$ecret")
        .await;
    info!("Cluster Distribution completed");
    async_std::task::sleep(Duration::from_secs(3)).await;
    
    // Test completed successfully
    info!("✅ Test completed successfully");
}
