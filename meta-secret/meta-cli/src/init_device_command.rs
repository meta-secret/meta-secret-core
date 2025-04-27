use anyhow::Result;
use anyhow::bail;
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::db::descriptors::creds::DeviceCredsDescriptor;
use tracing::info;
use crate::base_command::BaseCommand;

pub struct InitDeviceCommand {
    base: BaseCommand,
    pub device_name: String,
}

impl InitDeviceCommand {
    pub fn new(db_name: String, device_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
            device_name,
        }
    }
    
    pub async fn execute(&self) -> Result<()> {
        info!(
            "Generating device credentials for device: {}",
            self.device_name
        );

        // Create or open database
        let db_context = self.base.open_or_create_db().await?;

        // Check if device credentials already exist
        let maybe_device_creds = db_context.p_obj.find_tail_event(DeviceCredsDescriptor).await?;

        if maybe_device_creds.is_some() {
            bail!("{}", BaseCommand::already_exists_error("Device"));
        }

        // Generate device credentials since they don't exist
        let device_name = DeviceName::from(self.device_name.clone());
        let device_creds = db_context.p_creds.get_or_generate_device_creds(device_name).await?;

        println!("Meta Secret Initialization:");
        println!("---------------------------");
        println!("Device credentials generated successfully");
        println!();
        println!("Device Information:");
        println!(
            "  Device Name: {}",
            device_creds.device.device_name.as_str()
        );
        println!("  Device ID: {}", device_creds.device.device_id);

        Ok(())
    }
}
