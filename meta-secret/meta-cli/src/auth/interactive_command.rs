use crate::base_command::BaseCommand;
use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Input, Select};
use crate::auth::accept_all_join_requests_command::AcceptAllJoinRequestsCommand;
use crate::auth::accept_join_request_command::AcceptJoinRequestCommand;
use crate::auth::sign_up_command::JoinVaultCommand;

pub struct AuthInteractiveCommand {
    base: BaseCommand,
}

impl AuthInteractiveCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    pub async fn execute(&self) -> Result<()> {
        let items = vec!["Sign Up (Create/Join vault)", "Accept Join Request", "Accept All Join Requests"];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select authentication action")
            .default(0)
            .items(&items)
            .interact()?;

        match selection {
            0 => {
                // Sign Up
                let sign_up_cmd = JoinVaultCommand::new(self.base.db_name.clone());
                sign_up_cmd.execute().await?
            }
            1 => {
                // Accept Join Request
                let device_id = Input::<String>::new()
                    .with_prompt("Enter device ID")
                    .interact()?;
                
                let accept_cmd = AcceptJoinRequestCommand::new(self.base.db_name.clone(), device_id);
                accept_cmd.execute().await?
            }
            2 => {
                // Accept All Join Requests
                let accept_all_cmd = AcceptAllJoinRequestsCommand::new(self.base.db_name.clone());
                accept_all_cmd.execute().await?
            }
            _ => unreachable!(),
        }

        Ok(())
    }
} 