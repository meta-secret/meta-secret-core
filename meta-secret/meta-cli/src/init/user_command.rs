use crate::base_command::{BaseCommand, DbContext};
use anyhow::{Result, bail};
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;

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

        // Delegate to the generic execute method
        self.execute_with_context(&db_context).await
    }

    // Generic method that works with any DbContext
    pub async fn execute_with_context<Repo: KvLogEventRepo>(
        &self,
        db_context: &DbContext<Repo>,
    ) -> Result<()> {
        // Get device credentials (or fail if they don't exist)
        self.base.ensure_device_creds(db_context).await?;
        let device_creds = db_context.p_creds.get_device_creds().await?.unwrap();

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
        println!("Vault Name: {}", user_creds.vault_name.0);

        Ok(())
    }
}
