extern crate core;

use std::sync::Arc;
use anyhow::{Result};
use clap::{Parser, Subcommand};
use meta_secret_core::secret::data_block::common::SharedSecretConfig;
use serde::{Deserialize, Serialize};
use tracing::info;
use meta_db_redb::ReDbRepo;
use meta_secret_core::node::app::meta_app::meta_client_service::MetaClientService;
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;

#[derive(Debug, Parser)]
#[command(about = "Meta Secret Command Line Application", long_about = None)]
struct CmdLine {
    #[command(subcommand)]
    command: Command,
}

/// Simple program to greet a person
#[derive(Subcommand, Debug)]
enum Command {
    /// Generate device credentials
    Init {
        #[arg(short, long)]
        device_name: String,
    },
    Info,
}

#[derive(Debug, Serialize, Deserialize)]
struct MetaSecretConfig {
    shared_secret: SharedSecretConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .finish();
    let _guard = tracing::subscriber::set_global_default(subscriber)?;
    
    let args = CmdLine::parse();
    
    match args.command {
        Command::Init { device_name } => {
            info!("Generating device credentials for device: {}", device_name);
            let p_obj = Arc::new(ReDbRepo::??? open or create);
        }
        
        Command::Info => {
            let repo = Arc::new(InMemKvLogEventRepo::default());
            let p_obj = Arc::new(PersistentObject::new(repo.clone()));

            let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
            let maybe_user_creds = p_creds.get_user_creds().await?;

            let user_creds = maybe_user_creds else {
                println!("Fresh run, no user credentials found.");
                return Ok(())
            };
            
            let sync_protocol = HttpSyncProtocol {
                api_url: ApiUrl::prod(),
            };

            // let client_gw = Arc::new(SyncGateway {
            //     id: "mobile_client".to_string(),
            //     p_obj: p_obj.clone(),
            //     sync: Arc::new(sync_protocol),
            //     device_creds: Arc::new(user_creds.device_creds.clone())
            // });
            // 
            // let client = MetaClientService {
            //     data_transfer: Arc::new(MetaClientDataTransfer {}),
            //     sync_gateway: Arc::new(SyncGateway {}),
            //     state_provider: Arc::new(()),
            //     p_obj: Arc::new(PersistentObject {}),
            //     device_creds: Arc::new(DeviceCreds {}),
            // };
        }
    }
    
    Ok(())
}
