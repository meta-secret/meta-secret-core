use std::path::Path;
use std::sync::Arc;
use anyhow::{anyhow, bail, Result};
use meta_db_redb::ReDbRepo;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use meta_secret_core::node::db::descriptors::creds::UserCredsDescriptor;

pub struct InitUserCommand {
    pub db_name: String,
    pub vault_name: VaultName,
}

impl InitUserCommand {
    pub async fn execute(&self) -> Result<()> {
        // Check if the database exists
        let db_path = Path::new(&self.db_name);
        if !db_path.exists() {
            return Err(anyhow!(
                "Database does not exist. Please run `meta-secret init-device` first."
            ));
        }

        // Open database connection
        let repo = Arc::new(ReDbRepo::open(db_path)?);
        
        // Create persistent object and credentials manager
        let p_obj = Arc::new(PersistentObject::new(repo));
        let p_creds = PersistentCredentials { p_obj: p_obj.clone() };

        // Get device credentials (or fail if they don't exist)
        let device_creds = match p_creds.get_device_creds().await? {
            Some(creds) => creds.value(),
            None => {
                return Err(anyhow!("Device credentials not found. Please run `meta-secret init-device` first."));
            }
        };

        // Check if user credentials already exist
        let maybe_user_creds = p_creds.get_user_creds().await?;
        if maybe_user_creds.is_some() {
            let err = "User credentials already exist. Cannot initialize again.";
            let info = "Use the 'info' command to view existing credentials.";
            bail!("{} {}", err, info);
        }

        let device_name = device_creds.device.device_name.clone();
        
        // Generate user credentials
        let user_creds = p_creds
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