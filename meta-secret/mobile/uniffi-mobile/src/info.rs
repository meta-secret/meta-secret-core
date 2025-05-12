use std::sync::Arc;
use serde_json::json;
use tracing::info;
use uniffi::deps::anyhow;
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

pub fn get_app_info() -> String {
    sync_wrapper(internal_get_app_info()).unwrap_or_else(|e| json!({
            "success": false,
            "status": "Error",
            "message": format!("Error: {}", e)
        }).to_string())
}

async fn internal_get_app_info() -> anyhow::Result<String> {
    info!("Starting get_info async operation");

    let repo = Arc::new(InMemKvLogEventRepo::default());
    let p_obj = Arc::new(PersistentObject::new(repo.clone()));
    let p_creds = PersistentCredentials {
        p_obj: p_obj.clone(),
    };

    let maybe_device_creds = p_creds.get_device_creds().await?;
    if maybe_device_creds.is_none() {
        info!("Device isn't initialized");
        return Ok(json!({
                "success": false,
                "status": "NotInitialized",
                "message": "Device isn't initialized"
        }).to_string());
    }
    
    let maybe_user_creds = p_creds.get_user_creds().await?;
    if maybe_user_creds.is_none() {
        info!("Only device initialized, user missing");
        return Ok(json!({
                "success": false,
                "status": "DeviceOnly",
                "message": "Only device initialized, user missing"
        }).to_string());
    }

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
            return Ok(json!({
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
    let vault_status = p_vault.find(user_creds.user().clone()).await?;

    info!("Vault status: {:?}", vault_status);
    Ok(json!({
        "success": false,
        "status": vault_status,
        "message": "Vault status is ready"
    }).to_string())
}

