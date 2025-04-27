mod init_command;
mod info_command;

extern crate core;

use anyhow::Result;
use clap::{Parser, Subcommand};
use meta_secret_core::secret::data_block::common::SharedSecretConfig;
use serde::{Deserialize, Serialize};
use crate::init_command::InitCommand;
use crate::info_command::InfoCommand;

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
    
    let db_name = String::from("meta-secret.redb");
    
    match args.command {
        Command::Init { device_name } => {
            let init_cmd = InitCommand {
                db_name: db_name.clone(),
                device_name,
            };
            
            init_cmd.execute().await?
        }
        
        Command::Info => {
            let info_cmd = InfoCommand { db_name, };
            info_cmd.execute().await?
        }
    }
    
    Ok(())
}
