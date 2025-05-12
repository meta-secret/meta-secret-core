use crate::base_command::BaseCommand;
use crate::init::device_command::InitDeviceCommand;
use crate::init::user_command::InitUserCommand;
use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Input, Select};
use meta_secret_core::node::common::model::vault::vault::VaultName;
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};

#[derive(Debug, Clone, Copy, Display, EnumIter)]
pub enum InitOption {
    #[strum(to_string = "Device")]
    Device,
    #[strum(to_string = "User")]
    User,
    #[strum(to_string = "Back to Main Menu")]
    Back,
}

pub struct InitOptionSelector;

impl InitOptionSelector {
    pub fn select() -> Result<InitOption> {
        let options: Vec<InitOption> = InitOption::iter().collect();
        let items: Vec<String> = options.iter().map(|o| o.to_string()).collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select initialization type")
            .default(0)
            .items(&items)
            .interact()?;

        Ok(options[selection])
    }
}

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
        let option = InitOptionSelector::select()?;

        match option {
            InitOption::Device => {
                // Device initialization
                let device_name = Input::<String>::new()
                    .with_prompt("Enter device name")
                    .interact()?;

                let init_device_cmd =
                    InitDeviceCommand::new(self.base.db_name.clone(), device_name);
                init_device_cmd.execute().await?
            }
            InitOption::User => {
                // User initialization
                let vault_name_str = Input::<String>::new()
                    .with_prompt("Enter vault name")
                    .interact()?;

                let vault_name = VaultName::from(vault_name_str);
                let init_user_cmd = InitUserCommand::new(self.base.db_name.clone(), vault_name);
                init_user_cmd.execute().await?
            }
            InitOption::Back => {
                // Back to main menu
                println!("Returning to main menu");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_option_order_matches_selection_indices() {
        // Collect all InitOption variants in order
        let options: Vec<InitOption> = InitOption::iter().collect();

        // Verify the order matches expected indices
        assert_eq!(options.len(), 3);
        assert!(matches!(options[0], InitOption::Device));
        assert!(matches!(options[1], InitOption::User));
        assert!(matches!(options[2], InitOption::Back));
    }

    #[test]
    fn test_init_option_display_strings() {
        // Verify the Display implementation produces the correct strings
        assert_eq!(InitOption::Device.to_string(), "Device");
        assert_eq!(InitOption::User.to_string(), "User");
        assert_eq!(InitOption::Back.to_string(), "Back to Main Menu");
    }
}
