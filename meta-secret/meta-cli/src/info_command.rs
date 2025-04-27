use std::path::Path;
use std::sync::Arc;
use anyhow::{bail, Result};
use meta_db_redb::ReDbRepo;
use meta_secret_core::node::app::meta_app::meta_client_service::{MetaClientDataTransfer, MetaClientService, MetaClientStateProvider};
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;

pub struct InfoCommand {
    pub db_name: String,
}

impl InfoCommand {
    pub async fn execute(&self) -> Result<()> {
        // Database file path
        let db_path = Path::new(self.db_name.as_str());
        
        if !db_path.exists() {
            println!("No database found. Please run 'init' command first.");
            bail!("Database not found");
        }
        
        // Open existing database
        let repo = Arc::new(ReDbRepo::open(db_path)?);
        let p_obj = Arc::new(PersistentObject::new(repo));
        let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
        
        println!("Meta Secret Information:");
        println!("----------------------");
        
        // Try to get device credentials
        let maybe_device_creds = p_creds.get_device_creds().await?;
        
        match maybe_device_creds {
            Some(device_creds_event) => {
                let device_creds = device_creds_event.value();
                println!("Device Information:");
                println!("  Device ID: {}", device_creds.device.device_id);
                println!("  Device Name: {}", device_creds.device.device_name.as_str());
            }
            None => {
                println!("Not initialized. Run the 'init' command first.");
                return Ok(());
            }
        }
        
        let maybe_user_creds = p_creds.get_user_creds().await?;
        
        println!();
        let Some(user_creds) = maybe_user_creds else {
            println!("User Status: Device is initialized but not associated with a vault.");
            return Ok(());
        };

        let sync_protocol = HttpSyncProtocol {
            api_url: ApiUrl::prod(),
        };

        let device_creds = Arc::new(user_creds.device_creds.clone());
        let sync_gateway = Arc::new(SyncGateway {
            id: "meta-cli".to_string(),
            p_obj: p_obj.clone(),
            sync: Arc::new(sync_protocol),
            device_creds: device_creds.clone()
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());
        
        let client = MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer { dt: MpscDataTransfer::new() }),
            sync_gateway,
            state_provider,
            p_obj,
            device_creds,
        };

        let app_state = client.get_app_state().await?;
        
        Ok(())
    }
} 