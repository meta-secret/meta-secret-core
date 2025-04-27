mod init_command;

extern crate core;

use std::sync::Arc;
use std::path::Path;
use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use meta_secret_core::secret::data_block::common::SharedSecretConfig;
use serde::{Deserialize, Serialize};
use tracing::info;
use meta_db_redb::ReDbRepo;
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::db::descriptors::creds::DeviceCredsDescriptor;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::init_command::InitCommand;

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
    /// Show information about the device and credentials
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
    tracing::subscriber::set_global_default(subscriber)?;
    
    let args = CmdLine::parse();
    
    match args.command {
        Command::Init { device_name } => {
            let init_cmd = InitCommand {
                db_name: String::from("meta-secret.redb"),
                device_name,
            };
            
            init_cmd.execute().await?
        }
        
        Command::Info => {
            // Database file path
            let db_path = Path::new("meta-secret.redb");
            
            if !db_path.exists() {
                println!("No database found. Please run 'init' command first.");
                return Ok(());
            }
            
            // Open existing database
            let repo = Arc::new(ReDbRepo::open(db_path)?);
            let p_obj = Arc::new(PersistentObject::new(repo));
            let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
            
            // Try to get device credentials
            let maybe_device_creds = p_creds.get_user_creds().await?;
            
            match maybe_device_creds {
                Some(user_creds) => {
                    println!("Device information:");
                    println!("  Device ID: {}", user_creds.device_creds.device.device_id.to_string());
                    println!("  Device Name: {}", user_creds.device_creds.device.device_name.as_str());
                    println!("  Vault Name: {}", user_creds.vault_name.to_string());
                }
                None => {
                    println!("No user credentials found.");
                }
            }
        }
    }
    
    Ok(())
}
