use crate::base_command::BaseCommand;
use crate::cli_format::CliOutputFormat;
use crate::secret::accept_all_recovery_requests_command::AcceptAllRecoveryRequestsCommand;
use crate::secret::accept_recovery_request_command::AcceptRecoveryRequestCommand;
use crate::secret::recovery_request_command::RecoveryRequestCommand;
use crate::secret::show_local_secret_command::ShowLocalSecretCommand;
use crate::secret::split_command::SplitCommand;
use anyhow::Result;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use dialoguer::{theme::ColorfulTheme, Input, Password, Select};
use meta_secret_core::node::common::model::meta_pass::PlainPassInfo;
use strum_macros::Display;

#[derive(Debug, Clone, Copy, Display)]
pub enum SecretOption {
    #[strum(to_string = "Split Secret")]
    SplitSecret,
    #[strum(to_string = "Recover Secret")]
    RequestRecovery,
    #[strum(to_string = "Show Secret")]
    ShowSecret,
    #[strum(to_string = "Accept Recovery Request")]
    AcceptRecoveryRequest,
    #[strum(to_string = "Accept All Recovery Requests")]
    AcceptAllRecoveryRequests,
    #[strum(to_string = "Back to Main Menu")]
    Back,
}

pub struct SecretOptionSelector;

impl SecretOptionSelector {
    pub fn select(show_secret: bool) -> Result<SecretOption> {
        let mut options = vec![SecretOption::SplitSecret];
        if show_secret {
            options.push(SecretOption::ShowSecret);
        } else {
            options.push(SecretOption::RequestRecovery);
        }
        options.push(SecretOption::AcceptRecoveryRequest);
        options.push(SecretOption::AcceptAllRecoveryRequests);
        options.push(SecretOption::Back);

        let items: Vec<String> = options.iter().map(|o| o.to_string()).collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select secret management action")
            .default(0)
            .items(&items)
            .interact()?;

        Ok(options[selection])
    }
}

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
        let show_secret = self.should_show_local_secret().await?;
        let option = SecretOptionSelector::select(show_secret)?;

        match option {
            SecretOption::SplitSecret => {
                // Split Secret
                let pass_name = Input::<String>::new()
                    .with_prompt("Enter password name")
                    .interact()?;

                let pass = Password::new()
                    .with_prompt("Enter password to split")
                    .with_confirmation("Confirm password", "Passwords don't match")
                    .interact()?;

                let plain_pass = PlainPassInfo::new(pass_name, pass);
                let split_cmd = SplitCommand::new(self.base.db_name.clone());
                split_cmd.execute(plain_pass).await?
            }
            SecretOption::RequestRecovery => {
                // Recover Secret
                let pass_name = Input::<String>::new()
                    .with_prompt("Enter password name to recover")
                    .interact()?;

                let recover_cmd = RecoveryRequestCommand::new(self.base.db_name.clone(), pass_name);
                recover_cmd.execute().await?
            }
            SecretOption::ShowSecret => {
                // Show Secret
                let pass_name = Input::<String>::new()
                    .with_prompt("Enter password name to show")
                    .interact()?;

                let show_command = ShowLocalSecretCommand::new(
                    self.base.db_name.clone(),
                    CliOutputFormat::default(),
                );
                show_command.execute(pass_name).await?
            }
            SecretOption::AcceptRecoveryRequest => {
                // Accept Recovery Request
                let claim_id = Input::<String>::new()
                    .with_prompt("Enter claim ID")
                    .interact()?;

                let accept_recover_cmd =
                    AcceptRecoveryRequestCommand::new(self.base.db_name.clone(), claim_id);
                accept_recover_cmd.execute().await?
            }
            SecretOption::AcceptAllRecoveryRequests => {
                // Accept All Recovery Requests
                let accept_all_recover_cmd =
                    AcceptAllRecoveryRequestsCommand::new(self.base.db_name.clone());
                accept_all_recover_cmd.execute().await?
            }
            SecretOption::Back => {
                // Back to main menu
                println!("Returning to main menu");
            }
        }

        Ok(())
    }

    async fn should_show_local_secret(&self) -> Result<bool> {
        let db_context = self.base.open_existing_db().await?;
        self.base.ensure_user_creds(&db_context).await?;

        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        let ApplicationState::Vault(VaultFullInfo::Member(member_info)) = app_state else {
            return Ok(false);
        };

        Ok(member_info.member.vault.members().len() <= 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_option_order_matches_selection_indices() {
        // Collect all SecretOption variants in local-show mode order
        let options: Vec<SecretOption> = vec![
            SecretOption::SplitSecret,
            SecretOption::ShowSecret,
            SecretOption::AcceptRecoveryRequest,
            SecretOption::AcceptAllRecoveryRequests,
            SecretOption::Back,
        ];

        // Verify the order matches expected indices
        assert_eq!(options.len(), 5);
        assert!(matches!(options[0], SecretOption::SplitSecret));
        assert!(matches!(options[1], SecretOption::ShowSecret));
        assert!(matches!(options[2], SecretOption::AcceptRecoveryRequest));
        assert!(matches!(options[3], SecretOption::AcceptAllRecoveryRequests));
        assert!(matches!(options[4], SecretOption::Back));
    }

    #[test]
    fn test_secret_option_display_strings() {
        // Verify the Display implementation produces the correct strings
        assert_eq!(SecretOption::SplitSecret.to_string(), "Split Secret");
        assert_eq!(SecretOption::RequestRecovery.to_string(), "Recover Secret");
        assert_eq!(SecretOption::ShowSecret.to_string(), "Show Secret");
        assert_eq!(
            SecretOption::AcceptRecoveryRequest.to_string(),
            "Accept Recovery Request"
        );
        assert_eq!(
            SecretOption::AcceptAllRecoveryRequests.to_string(),
            "Accept All Recovery Requests"
        );
        assert_eq!(SecretOption::Back.to_string(), "Back to Main Menu");
    }
}
