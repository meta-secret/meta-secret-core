use crate::base_command::BaseCommand;
use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Input, Select};
use meta_secret_core::node::common::model::vault::vault::VaultName;
use crate::init::device_command::InitDeviceCommand;
use crate::init::user_command::InitUserCommand;

pub struct InitInteractiveCommand {
    base: BaseCommand,
}

impl InitInteractiveCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    pub async fn execute(&self) -> Result<()> {
        let items = vec!["Device", "User", "Back to Main Menu"];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select initialization type")
            .default(0)
            .items(&items)
            .interact()?;

        match selection {
            0 => {
                // Device initialization
                let device_name = Input::<String>::new()
                    .with_prompt("Enter device name")
                    .interact()?;
                
                let init_device_cmd = InitDeviceCommand::new(self.base.db_name.clone(), device_name);
                init_device_cmd.execute().await?
            }
            1 => {
                // User initialization
                let vault_name_str = Input::<String>::new()
                    .with_prompt("Enter vault name")
                    .interact()?;
                
                let vault_name = VaultName::from(vault_name_str);
                let init_user_cmd = InitUserCommand::new(self.base.db_name.clone(), vault_name);
                init_user_cmd.execute().await?
            }
            2 => {
                // Back to main menu
                println!("Returning to main menu");
            }
            _ => unreachable!(),
        }

        Ok(())
    }
} 