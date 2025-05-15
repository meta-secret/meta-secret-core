use crate::base_command::{BaseCommand, DbContext};
use anyhow::bail;
use anyhow::Result;
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::db::descriptors::creds::DeviceCredsDescriptor;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use tracing::info;

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

        // Delegate to the generic execute method
        self.execute_with_context(&db_context).await
    }

    // Generic method that works with any DbContext
    pub async fn execute_with_context<Repo: KvLogEventRepo>(
        &self,
        db_context: &DbContext<Repo>,
    ) -> Result<()> {
        // Check if device credentials already exist
        let maybe_device_creds = db_context
            .p_obj
            .find_tail_event(DeviceCredsDescriptor)
            .await?;

        if maybe_device_creds.is_some() {
            bail!("{}", BaseCommand::already_exists_error("Device"));
        }

        // Generate device credentials since they don't exist
        let device_name = DeviceName::from(self.device_name.clone());
        let device_creds = db_context
            .p_creds
            .get_or_generate_device_creds(device_name)
            .await?;

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

#[cfg(test)]
pub mod tests {
    use super::*;
    use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
    use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
    use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
    use std::sync::Arc;
    use meta_secret_core::crypto::key_pair::{KeyPair, TransportDsaKeyPair};

    #[tokio::test]
    async fn test_init_device_command() -> Result<()> {
        // Create an in-memory database context
        let db_context = create_in_memory_context().await;

        let device_name = "device_a";

        // Create the command instance
        let init_device_cmd = InitDeviceCommand::new(
            "in_memory_db".to_string(), // This name is just for consistency, not used for actual file
            device_name.to_string(),
        );

        // Execute the command with our in-memory context
        let result = init_device_cmd.execute_with_context(&db_context).await;

        // Verify the command succeeded
        assert!(
            result.is_ok(),
            "Command should succeed but failed with: {:?}",
            result
        );

        // Verify that device credentials were created correctly
        let maybe_device_creds = db_context
            .p_obj
            .find_tail_event(DeviceCredsDescriptor)
            .await?;
        assert!(
            maybe_device_creds.is_some(),
            "Device credentials should exist after initialization"
        );

        // Verify the device name is correct
        let device_creds = maybe_device_creds.unwrap();
        assert_eq!(
            device_creds.value().device.device_name.as_str(),
            device_name,
            "Device name should match the input value"
        );

        // Try to run the command again - should fail because credentials already exist
        let result = init_device_cmd.execute_with_context(&db_context).await;
        assert!(
            result.is_err(),
            "Second execution should fail but succeeded"
        );

        Ok(())
    }

    pub async fn create_in_memory_context() -> DbContext<InMemKvLogEventRepo> {
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));
        
        // Always use the same key for tests to avoid "Invalid recipient" errors
        let key_pair = TransportDsaKeyPair::generate();
        let master_key = key_pair.sk();
        
        // Create persistent credentials with this master key
        let p_creds = PersistentCredentials {
            p_obj: p_obj.clone(),
            master_key,
        };

        DbContext {
            repo,
            p_obj,
            p_creds,
        }
    }
}
