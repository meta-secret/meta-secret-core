mod info_command;
mod init_device_command;
mod init_user_command;
mod base_command;

extern crate core;

use crate::info_command::InfoCommand;
use crate::init_device_command::InitDeviceCommand;
use crate::init_user_command::InitUserCommand;
use anyhow::Result;
use clap::{Parser, Subcommand};
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::secret::data_block::common::SharedSecretConfig;
use serde::{Deserialize, Serialize};

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
    InitDevice {
        #[arg(short, long)]
        device_name: String,
    },
    /// Generate user credentials
    InitUser {
        #[arg(short, long)]
        vault_name: VaultName,
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
        Command::InitDevice { device_name } => {
            let init_device_cmd = InitDeviceCommand::new(db_name.clone(), device_name);
            init_device_cmd.execute().await?
        }

        Command::InitUser { vault_name } => {
            let init_user_cmd = InitUserCommand::new(db_name.clone(), vault_name);
            init_user_cmd.execute().await?
        }

        Command::Info => {
            let info_cmd = InfoCommand::new(db_name);
            info_cmd.execute().await?
        }
    }

    Ok(())
}
