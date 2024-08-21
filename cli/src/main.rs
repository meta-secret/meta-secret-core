extern crate core;

use std::fs::File;
use std::string::FromUtf8Error;

use anyhow::{Context, Result};
use clap::{ArgEnum, Parser, Subcommand};
use meta_secret_core::shared_secret::data_block::common::SharedSecretConfig;
use meta_secret_core::{
    convert_qr_images_to_json_files, recover, split, CoreResult, RecoveryOperationError,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Parser)]
#[clap(about = "Meta Secret Command Line Application", long_about = None)]
struct CmdLine {
    #[clap(subcommand)]
    command: Command,
}

/// Simple program to greet a person
#[derive(Subcommand, Debug)]
enum Command {
    Split {
        #[clap(short, long)]
        secret: String,
    },
    Restore {
        #[clap(short, long, arg_enum)]
        from: RestoreType,
    },
}

#[derive(Debug, Clone, ArgEnum, Eq, PartialEq)]
#[clap(rename_all = "kebab_case")]
enum RestoreType {
    Qr,
    Json,
}

#[derive(Debug, Serialize, Deserialize)]
struct MetaSecretConfig {
    shared_secret: SharedSecretConfig,
}

///https://kerkour.com/rust-cross-compilation
fn main() -> Result<()> {
    let args: CmdLine = CmdLine::parse();

    let config_file = File::open("config.yaml")
        .with_context(|| "Error reading config.yaml. Please check that file exists.")?;

    let app_config: MetaSecretConfig = serde_yaml::from_reader(config_file)
        .with_context(|| "Error parsing config file. Invalid yaml format")?;

    let shared_secret_config = app_config.shared_secret;

    match args.command {
        Command::Split { secret } => {
            split(secret, shared_secret_config).with_context(|| "Error splitting password")?
        }
        Command::Restore { from } => match from {
            RestoreType::Qr => {
                convert_qr_images_to_json_files()
                    .with_context(|| "Error converting qr codes into json files")?;

                let password = restore_from_json().with_context(|| "Can't restore password")?;

                println!("Restored password: {:?}", password);
            }
            RestoreType::Json => {
                let password = restore_from_json()?;
                println!("Restored password: {:?}", password);
            }
        },
    }

    println!("Finished");
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    /// https://dailydevsblog.com/troubleshoot/resolved-issue-with-a-string-and-thiserror-as_dyn_error-exists-for-reference-string-but-its-trait-bounds-were-not-satisfied-in-rust-139876/
    #[error(transparent)]
    RecoveryError(#[from] RecoveryOperationError),
    #[error("Error parse binary data. Non utf8 encoding.")]
    ParsingError {
        #[from]
        source: FromUtf8Error,
    },
}

fn restore_from_json() -> CoreResult<String> {
    let text = recover()?;
    let password = text.text;
    Ok(password)
}
