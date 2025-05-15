use std::sync::Arc;
use serde_json::json;
use tracing::info;
use std::future::Future;
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::objects::persistent_vault::PersistentVault;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;

fn sync_wrapper<F: Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(future)
}

#[uniffi::export]
pub fn get_app_info() -> String {
    sync_wrapper(async {
        internal_get_app_info().await.unwrap_or_else(|| json!({
                "success": false,
                "status": "Error",
                "message": "Failed to get app info"
            }).to_string())
    })
}

async fn internal_get_app_info() -> Option<String> {
    info!("Starting get_info async operation");

    let repo = Arc::new(InMemKvLogEventRepo::default());
    let p_obj = Arc::new(PersistentObject::new(repo.clone()));
    let p_creds = PersistentCredentials {
        p_obj: p_obj.clone(),
    };

    let maybe_device_creds = match p_creds.get_device_creds().await {
        Ok(device_creds) => {device_creds}
        Err(e) => {
            info!("Device isn't initialized: {:?}", e);
            return Some(json!({
                "success": false,
                "status": "NotInitialized",
                "message": format!("Device isn't initialized: {:?}", e)
            }).to_string());
        }
    };

    let maybe_user_creds = match p_creds.get_user_creds().await {
        Ok(user_creds) => {user_creds}
        Err(e) => {
            info!("Only device initialized, user missing: {:?}", e);
            return Some(json!({
                "success": false,
                "status": "DeviceOnly",
                "message": format!("User isn't initialized: {:?}", e)
            }).to_string());
        }
    };
    
    let user_creds = maybe_user_creds.unwrap();
    let device_creds = maybe_device_creds.unwrap().value();

    // TODO: Need to place it out
    let sync_protocol = HttpSyncProtocol {
        api_url: ApiUrl::prod(),
    };
    let client_gw = Arc::new(SyncGateway {
        id: "mobile_client".to_string(),
        p_obj: p_obj.clone(),
        sync: Arc::new(sync_protocol),
        device_creds: Arc::new(device_creds.clone()),
    });

    match client_gw.sync(user_creds.user()).await {
        Err(e) => {
            info!("Sync failed: {}", e);
            return Some(json!({
                    "success": false,
                    "status": "Offline",
                    "message": format!("Sync failed: {}", e),
                    "device": {
                        "id": device_creds.device.device_id,
                        "name": device_creds.device.device_name.as_str()
                    },
                    "user": {
                        "vault_name": user_creds.vault_name
                    }
                }).to_string());
        },
        _ => info!("Sync completed successfully")
    }

    let p_vault = Arc::new(PersistentVault::from(p_obj.clone()));
    match p_vault.find(user_creds.user().clone()).await {
        Ok(vault_status) => {
            info!("Vault status: {:?}", vault_status);
            Some(json!({
                "success": true,
                "status": vault_status,
                "message": "Vault status is ready"
            }).to_string())
        }
        Err(e) => {
            info!("Vault status error: {:?}", e);
            Some(json!({
                "success": false,
                "status": "NoVaultStatus",
                "message": format!("Vault status error: {:?}", e),
            }).to_string())
        }
    }
}

uniffi::setup_scaffolding!();