use crate::base_command::BaseCommand;
use crate::init::interactive_command::InitInteractiveCommand;
use crate::auth::interactive_command::AuthInteractiveCommand;
use crate::secret::interactive_command::SecretInteractiveCommand;
use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Select};
use crate::info::info_command::InfoCommand;

pub struct InteractiveCommand {
    base: BaseCommand,
}

impl InteractiveCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    pub async fn execute(&self) -> Result<()> {
        let items = vec!["Initialize", "Authentication", "Secret Management", "Show Device Info"];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select category")
            .default(0)
            .items(&items)
            .interact()?;

        match selection {
            0 => {
                let init_cmd = InitInteractiveCommand::new(self.base.db_name.clone());
                init_cmd.execute().await?
            },
            1 => {
                let auth_cmd = AuthInteractiveCommand::new(self.base.db_name.clone());
                auth_cmd.execute().await?
            },
            2 => {
                let secret_cmd = SecretInteractiveCommand::new(self.base.db_name.clone());
                secret_cmd.execute().await?
            },
            3 => {
                let info_cmd = InfoCommand::new(self.base.db_name.clone());
                info_cmd.execute().await?
            },
            _ => unreachable!(),
        }

        Ok(())
    }
} 