mod base_command;
mod info;
mod init;
mod auth;
mod secret;

extern crate core;

use crate::info::InfoCommand;
use crate::init::{InitDeviceCommand, InitUserCommand};
use crate::auth::{JoinVaultCommand, AcceptJoinRequestCommand, AcceptAllJoinRequestsCommand};
use crate::secret::{RecoveryRequestCommand, ShowSecretCommand, SplitCommand};
use anyhow::Result;
use clap::{Parser, Subcommand};
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::secret::data_block::common::SharedSecretConfig;
use serde::{Deserialize, Serialize};
use meta_secret_core::node::common::model::meta_pass::PlainPassInfo;

#[derive(Debug, Parser)]
#[command(about = "Meta Secret CLI", long_about = None)]
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
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    Secret {
        #[command(subcommand)]
        command: SecretCommand,
    },
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
enum AuthCommand {
    /// Create or Join a vault
    SignUp,
    AcceptJoinRequest {
        #[arg(long)]
        device_id: String,
    },
    /// Accept all pending join requests
    AcceptAllJoinRequests,
}

#[derive(Subcommand, Debug)]
enum SecretCommand {
    Split {
        #[arg(long)]
        pass: String,
        #[arg(long)]
        pass_name: String,
    },
    RecoveryRequest {
        #[arg(long)]
        pass_name: String,
    },
    Show {
        #[arg(long)]
        claim_id: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct MetaSecretConfig {
    shared_secret: SharedSecretConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
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
        Command::Auth { command } => {
            match command { 
                AuthCommand::SignUp => {
                    let sign_up_cmd = JoinVaultCommand::new(db_name);
                    sign_up_cmd.execute().await?
                },
                AuthCommand::AcceptJoinRequest { device_id } => {
                    let accept_cmd = AcceptJoinRequestCommand::new(db_name, device_id);
                    accept_cmd.execute().await?
                },
                AuthCommand::AcceptAllJoinRequests => {
                    let accept_all_cmd = AcceptAllJoinRequestsCommand::new(db_name);
                    accept_all_cmd.execute().await?
                }
            }
        }
        Command::Secret { command } => match command {
            SecretCommand::Split { pass, pass_name } => {
                let plain_pass = PlainPassInfo::new(pass_name, pass);
                let split_cmd = SplitCommand::new(db_name);
                split_cmd.execute(plain_pass).await?
            }
            SecretCommand::RecoveryRequest { pass_name } => {
                let recover_cmd = RecoveryRequestCommand::new(db_name, pass_name);
                recover_cmd.execute().await?
            }
            SecretCommand::Show { claim_id } => {
                let show_command = ShowSecretCommand::new(db_name);
                show_command.execute(claim_id).await?;
            }
        },
    }

    Ok(())
}
