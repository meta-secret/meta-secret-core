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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::device_command::InitDeviceCommand;
    use crate::init::device_command::tests::create_in_memory_context;
    use meta_secret_core::node::db::descriptors::creds::{
        DeviceCredsDescriptor, UserCredsDescriptor,
    };

    #[tokio::test]
    async fn test_init_user_command() -> Result<()> {
        // Create an in-memory database context
        let db_context = create_in_memory_context().await;
        
        let device_name = "device_a";
        let vault_name = VaultName::from("test_vault");
        
        let init_device_cmd =
            InitDeviceCommand::new("in_memory_db".to_string(), device_name.to_string());

        // Execute the device command with our in-memory context
        let device_result = init_device_cmd.execute_with_context(&db_context).await;
        assert!(
            device_result.is_ok(),
            "Device initialization should succeed but failed with: {:?}",
            device_result
        );

        // Verify that device credentials were created correctly
        let maybe_device_creds = db_context
            .p_obj
            .find_tail_event(DeviceCredsDescriptor)
            .await?;
        assert!(
            maybe_device_creds.is_some(),
            "Device credentials should exist after device initialization"
        );

        // Create the user command instance
        let init_user_cmd = InitUserCommand::new("in_memory_db".to_string(), vault_name.clone());

        // Use the proper execute_with_context method
        let result = init_user_cmd.execute_with_context(&db_context).await;

        // Verify the command succeeded
        assert!(
            result.is_ok(),
            "User initialization command should succeed but failed with: {:?}",
            result
        );

        // Verify that user credentials were created correctly
        let maybe_user_creds = db_context
            .p_obj
            .find_tail_event(UserCredsDescriptor)
            .await?;
        assert!(
            maybe_user_creds.is_some(),
            "User credentials should exist after user initialization"
        );

        // Verify the vault name is correct
        let user_creds = maybe_user_creds.unwrap().value();
        assert_eq!(
            user_creds.vault_name.0, vault_name.0,
            "Vault name should match the input value"
        );

        // Try to run the command again - should fail because user credentials already exist
        let second_result = init_user_cmd.execute_with_context(&db_context).await;
        assert!(
            second_result.is_err(),
            "Second execution should fail but succeeded"
        );

        Ok(())
    }
}
