mod base_command;
mod info_command;
mod init_device_command;
mod init_user_command;
mod join_vault_command;
mod recover_command;
mod split_command;

extern crate core;

use crate::info_command::InfoCommand;
use crate::init_device_command::InitDeviceCommand;
use crate::init_user_command::InitUserCommand;
use crate::join_vault_command::JoinVaultCommand;
use crate::recover_command::RecoverCommand;
use crate::split_command::SplitCommand;
use anyhow::Result;
use clap::{Parser, Subcommand};
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::secret::data_block::common::SharedSecretConfig;
use serde::{Deserialize, Serialize};
use meta_secret_core::node::common::model::meta_pass::PlainPassInfo;

#[derive(Debug, Parser)]
#[command(about = "Meta Secret Command Line Application", long_about = None)]
struct CmdLine {
    #[command(subcommand)]
    command: Command,
}

/// Simple program to greet a person
#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize device and user credentials
    Init {
        #[command(subcommand)]
        command: InitCommand,
    },
    Secret {
        #[command(subcommand)]
        command: SecretCommand,
    },
    /// Create or Join a vault
    SignUp,
    /// Show information about the device and credentials
    Info,
}

#[derive(Subcommand, Debug)]
enum InitCommand {
    /// Generate device credentials
    Device {
        #[arg(long)]
        device_name: String,
    },
    /// Generate user credentials
    User {
        #[arg(long)]
        vault_name: VaultName,
    },
}

#[derive(Subcommand, Debug)]
enum SecretCommand {
    Split {
        #[arg(long)]
        pass: String,
        #[arg(long)]
        pass_name: String,
    },
    Recover {
        #[arg(long)]
        pass_name: String,
    },
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
        Command::Init { command } => match command {
            InitCommand::Device { device_name } => {
                let init_device_cmd = InitDeviceCommand::new(db_name.clone(), device_name);
                init_device_cmd.execute().await?
            }
            InitCommand::User { vault_name } => {
                let init_user_cmd = InitUserCommand::new(db_name.clone(), vault_name);
                init_user_cmd.execute().await?
            }
        },
        Command::Info => {
            let info_cmd = InfoCommand::new(db_name);
            info_cmd.execute().await?
        }
        Command::SignUp => {
            let sign_up_cmd = JoinVaultCommand::new(db_name);
            sign_up_cmd.execute().await?
        }
        Command::Secret { command } => match command {
            SecretCommand::Split { pass, pass_name } => {
                let plain_pass = PlainPassInfo::new(pass_name, pass);
                let split_cmd = SplitCommand::new(db_name);
                split_cmd.execute(plain_pass).await?
            }
            SecretCommand::Recover { pass_name } => {
                let recover_cmd = RecoverCommand::new(db_name, pass_name);
                recover_cmd.execute().await?
            }
        },
    }

    Ok(())
}
