use crate::base_command::BaseCommand;
use anyhow::{Result, bail};
use meta_secret_core::node::common::model::vault::vault::VaultName;

pub struct InitUserCommand {
    base: BaseCommand,
    pub vault_name: VaultName,
}

impl InitUserCommand {
    pub fn new(db_name: String, vault_name: VaultName) -> Self {
        Self {
            base: BaseCommand::new(db_name),
            vault_name,
        }
    }

    pub async fn execute(&self) -> Result<()> {
        // Open existing database
        let db_context = self.base.open_existing_db().await?;

        // Get device credentials (or fail if they don't exist)
        self.base.ensure_device_creds(&db_context).await?;
        let device_creds = db_context.p_creds.get_device_creds().await?.unwrap().value();

        // Check if user credentials already exist
        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;
        if maybe_user_creds.is_some() {
            bail!("{}", BaseCommand::already_exists_error("User"));
        }

        let device_name = device_creds.device.device_name.clone();

        // Generate user credentials
        let user_creds = db_context
            .p_creds
            .get_or_generate_user_creds(device_name.clone(), self.vault_name.clone())
            .await?;

        // Print information about user credentials
        println!("User credentials for vault successfully configured");
        println!("Device ID: {}", device_creds.device.device_id);
        println!("Device Name: {:?}", device_name);
        println!("Vault Name: {}", self.vault_name);
        println!("User ID: {:?}", user_creds.user_id());

        Ok(())
    }
}
