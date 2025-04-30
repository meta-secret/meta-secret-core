extern crate core;
mod base_command;
mod info;
mod init;
mod auth;
mod secret;
mod interactive_command;

use crate::auth::accept_all_join_requests_command::AcceptAllJoinRequestsCommand;
use crate::auth::accept_join_request_command::AcceptJoinRequestCommand;
use crate::auth::interactive_command::AuthInteractiveCommand;
use crate::auth::sign_up_command::JoinVaultCommand;
use crate::info::info_command::InfoCommand;
use crate::init::device_command::InitDeviceCommand;
use crate::init::interactive_command::InitInteractiveCommand;
use crate::init::user_command::InitUserCommand;
use crate::interactive_command::InteractiveCommand;
use crate::secret::accept_all_recovery_requests_command::AcceptAllRecoveryRequestsCommand;
use crate::secret::accept_recovery_request_command::AcceptRecoveryRequestCommand;
use crate::secret::interactive_command::SecretInteractiveCommand;
use crate::secret::recovery_request_command::RecoveryRequestCommand;
use crate::secret::show_secret_command::ShowSecretCommand;
use crate::secret::split_command::SplitCommand;
use anyhow::Result;
use clap::{Parser, Subcommand};
use dialoguer::Password;
use meta_secret_core::node::common::model::meta_pass::PlainPassInfo;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use std::io::{self, IsTerminal, Read};

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
    Info {
        #[command(subcommand)]
        command: Option<InfoSubCommand>,
    },
    /// Fully interactive mode
    Interactive,
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
    /// Interactive mode for initialization
    Interactive,
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
    /// Interactive mode for authentication
    Interactive,
}

#[derive(Subcommand, Debug)]
enum SecretCommand {
    /// Split a password securely
    Split {
        /// Password name
        #[arg(long)]
        pass_name: String,
        
        /// Read password from stdin (pipe) instead of prompting
        #[arg(long)]
        stdin: bool,
    },
    RecoveryRequest {
        #[arg(long)]
        pass_name: String,
    },
    Show {
        #[arg(long)]
        claim_id: String,
    },
    AcceptRecoveryRequest {
        #[arg(long)]
        claim_id: String,
    },
    /// Accept all pending recovery requests
    AcceptAllRecoveryRequests,
    /// Interactive mode for secret management
    Interactive,
}

#[derive(Subcommand, Debug)]
enum InfoSubCommand {
    /// Show information about recovery claims
    RecoveryClaims,
    /// Show information about secrets in the vault
    Secrets,
    /// Show information about vault events
    VaultEvents,
}

/// Read password securely from stdin
fn read_password_from_stdin() -> Result<String> {
    let mut password = String::new();
    io::stdin().read_to_string(&mut password)?;
    // Remove trailing newline if present
    if password.ends_with('\n') {
        password.pop();
    }
    if password.ends_with('\r') {
        password.pop();
    }
    Ok(password)
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
            InitCommand::Interactive => {
                let init_interactive_cmd = InitInteractiveCommand::new(db_name.clone());
                init_interactive_cmd.execute().await?
            }
        },
        Command::Info { command } => {
            let info_cmd = InfoCommand::new(db_name);
            match command {
                Some(sub_command) => match sub_command {
                    InfoSubCommand::RecoveryClaims => info_cmd.show_recovery_claims().await?,
                    InfoSubCommand::Secrets => info_cmd.show_secrets().await?,
                    InfoSubCommand::VaultEvents => info_cmd.show_vault_events().await?,
                },
                None => info_cmd.execute().await?,
            }
        },
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
                },
                AuthCommand::Interactive => {
                    let auth_interactive_cmd = AuthInteractiveCommand::new(db_name);
                    auth_interactive_cmd.execute().await?
                }
            }
        },
        Command::Secret { command } => match command {
            SecretCommand::Split { pass_name, stdin } => {
                let pass = if stdin {
                    // Read password from stdin (pipe)
                    read_password_from_stdin()?
                } else if io::stdin().is_terminal() {
                    // Terminal is interactive, use secure password input
                    Password::new()
                        .with_prompt("Enter password to split")
                        .with_confirmation("Confirm password", "Passwords don't match")
                        .interact()?
                } else {
                    // Non-interactive but not explicitly set to stdin mode
                    eprintln!("No terminal detected for password input. Use --stdin flag to read from stdin.");
                    return Ok(());
                };
                
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
            SecretCommand::AcceptRecoveryRequest { claim_id } => {
                let accept_recover_cmd = AcceptRecoveryRequestCommand::new(db_name, claim_id);
                accept_recover_cmd.execute().await?
            }
            SecretCommand::AcceptAllRecoveryRequests => {
                let accept_all_recover_cmd = AcceptAllRecoveryRequestsCommand::new(db_name);
                accept_all_recover_cmd.execute().await?
            }
            SecretCommand::Interactive => {
                let secret_interactive_cmd = SecretInteractiveCommand::new(db_name);
                secret_interactive_cmd.execute().await?
            }
        },
        Command::Interactive => {
            let interactive_cmd = InteractiveCommand::new(db_name);
            interactive_cmd.execute().await?
        }
    }

    Ok(())
}
