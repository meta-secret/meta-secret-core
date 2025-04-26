extern crate core;

use anyhow::{Result};
use clap::{Parser, Subcommand};
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
    Info,
}

#[derive(Debug, Serialize, Deserialize)]
struct MetaSecretConfig {
    shared_secret: SharedSecretConfig,
}

fn main() -> Result<()> {
    let args = CmdLine::parse();
    
    match args.command {
        Command::Info => {
            println!("Meta Secret CLI Info");
        }
    }
    
    Ok(())
}
