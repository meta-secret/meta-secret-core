use crate::base_command::BaseCommand;
use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Input, Password, Select};
use meta_secret_core::node::common::model::meta_pass::PlainPassInfo;
use crate::secret::accept_all_recovery_requests_command::AcceptAllRecoveryRequestsCommand;
use crate::secret::accept_recovery_request_command::AcceptRecoveryRequestCommand;
use crate::secret::recovery_request_command::RecoveryRequestCommand;
use crate::secret::show_secret_command::ShowSecretCommand;
use crate::secret::split_command::SplitCommand;

pub struct SecretInteractiveCommand {
    base: BaseCommand,
}

impl SecretInteractiveCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    pub async fn execute(&self) -> Result<()> {
        let items = vec![
            "Split Secret", 
            "Request Recovery", 
            "Show Secret", 
            "Accept Recovery Request", 
            "Accept All Recovery Requests"
        ];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select secret management action")
            .default(0)
            .items(&items)
            .interact()?;

        match selection {
            0 => {
                // Split Secret
                let pass_name = Input::<String>::new()
                    .with_prompt("Enter password name")
                    .interact()?;
                
                let pass = Password::new()
                    .with_prompt("Enter password to split")
                    .interact()?;
                
                let plain_pass = PlainPassInfo::new(pass_name, pass);
                let split_cmd = SplitCommand::new(self.base.db_name.clone());
                split_cmd.execute(plain_pass).await?
            }
            1 => {
                // Request Recovery
                let pass_name = Input::<String>::new()
                    .with_prompt("Enter password name to recover")
                    .interact()?;
                
                let recover_cmd = RecoveryRequestCommand::new(self.base.db_name.clone(), pass_name);
                recover_cmd.execute().await?
            }
            2 => {
                // Show Secret
                let claim_id = Input::<String>::new()
                    .with_prompt("Enter claim ID")
                    .interact()?;
                
                let show_command = ShowSecretCommand::new(self.base.db_name.clone());
                show_command.execute(claim_id).await?
            }
            3 => {
                // Accept Recovery Request
                let claim_id = Input::<String>::new()
                    .with_prompt("Enter claim ID")
                    .interact()?;
                
                let accept_recover_cmd = AcceptRecoveryRequestCommand::new(self.base.db_name.clone(), claim_id);
                accept_recover_cmd.execute().await?
            }
            4 => {
                // Accept All Recovery Requests
                let accept_all_recover_cmd = AcceptAllRecoveryRequestsCommand::new(self.base.db_name.clone());
                accept_all_recover_cmd.execute().await?
            }
            _ => unreachable!(),
        }

        Ok(())
    }
} 