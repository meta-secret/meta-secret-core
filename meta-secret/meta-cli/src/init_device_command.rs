use std::path::Path;
use std::sync::Arc;
use anyhow::bail;
use tracing::info;
use meta_db_redb::ReDbRepo;
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::db::descriptors::creds::DeviceCredsDescriptor;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use anyhow::Result;

pub struct InitDeviceCommand {
    pub db_name: String,
    pub device_name: String
}

impl InitDeviceCommand {
    pub async fn execute(&self) -> Result<()> {
        info!("Generating device credentials for device: {}", self.device_name);

        // Database file path
        let db_path = Path::new(self.db_name.as_str());

        // Check if database exists and either open or create it
        let repo = if db_path.exists() {
            info!("Opening existing database at {}", db_path.display());
            Arc::new(ReDbRepo::open(db_path)?)
        } else {
            info!("Creating new database at {}", db_path.display());
            Arc::new(ReDbRepo::new(db_path)?)
        };

        // Create persistent object and credentials manager
        let p_obj = Arc::new(PersistentObject::new(repo));
        let p_creds = PersistentCredentials { p_obj: p_obj.clone() };

        // Check if device credentials already exist
        let maybe_device_creds = p_obj.find_tail_event(DeviceCredsDescriptor).await?;

        if maybe_device_creds.is_some() {
            let err = "Device credentials already exist. Cannot initialize again.";
            let info = "Use the 'info' command to view existing credentials.";
            bail!("{} {}", err, info);
        }

        // Generate device credentials since they don't exist
        let device_name = DeviceName::from(self.device_name.clone());
        let device_creds = p_creds.get_or_generate_device_creds(device_name).await?;

        println!("Meta Secret Initialization:");
        println!("---------------------------");
        println!("Device credentials generated successfully");
        println!();
        println!("Device Information:");
        println!("  Device Name: {}", device_creds.device.device_name.as_str());
        println!("  Device ID: {}", device_creds.device.device_id);
        
        Ok(())
    }
} 