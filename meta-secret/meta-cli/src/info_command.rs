use std::path::Path;
use std::sync::Arc;
use anyhow::{bail, Result};
use meta_db_redb::ReDbRepo;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;

pub struct InfoCommand {
    pub db_name: String,
}

impl InfoCommand {
    pub async fn execute(&self) -> Result<()> {
        // Database file path
        let db_path = Path::new(self.db_name.as_str());
        
        if !db_path.exists() {
            println!("No database found. Please run 'init' command first.");
            bail!("Database not found");
        }
        
        // Open existing database
        let repo = Arc::new(ReDbRepo::open(db_path)?);
        let p_obj = Arc::new(PersistentObject::new(repo));
        let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
        
        println!("Meta Secret Information:");
        println!("----------------------");
        
        // Try to get device credentials
        let maybe_device_creds = p_creds.get_device_creds().await?;
        
        match maybe_device_creds {
            Some(device_creds_event) => {
                let device_creds = device_creds_event.value();
                println!("Device Information:");
                println!("  Device ID: {}", device_creds.device.device_id);
                println!("  Device Name: {}", device_creds.device.device_name.as_str());
            }
            None => {
                println!("Not initialized. Run the 'init' command first.");
                return Ok(());
            }
        }
        
        let maybe_user_creds = p_creds.get_user_creds().await?;
        
        println!();
        match maybe_user_creds {
            Some(user_creds) => {
                println!("User Information:");
                println!("  Vault Name: {}", user_creds.vault_name);
                println!("  User Device ID: {}", user_creds.device_id());
            }
            None => {
                println!("User Status: Device is initialized but not associated with a user.");
            }
        }
        
        Ok(())
    }
} 