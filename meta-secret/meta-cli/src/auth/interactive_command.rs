use crate::auth::accept_all_join_requests_command::AcceptAllJoinRequestsCommand;
use crate::auth::accept_join_request_command::AcceptJoinRequestCommand;
use crate::auth::sign_up_command::JoinVaultCommand;
use crate::base_command::BaseCommand;
use anyhow::Result;
use dialoguer::{Input, Select, theme::ColorfulTheme};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};

#[derive(Debug, Clone, Copy, Display, EnumIter)]
pub enum AuthOption {
    #[strum(to_string = "Sign Up (Create/Join vault)")]
    SignUp,
    #[strum(to_string = "Accept Join Request")]
    AcceptJoinRequest,
    #[strum(to_string = "Accept All Join Requests")]
    AcceptAllJoinRequests,
    #[strum(to_string = "Back to Main Menu")]
    Back,
}

pub struct AuthOptionSelector;

impl AuthOptionSelector {
    pub fn select() -> Result<AuthOption> {
        let options: Vec<AuthOption> = AuthOption::iter().collect();
        let items: Vec<String> = options.iter().map(|o| o.to_string()).collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select authentication action")
            .default(0)
            .items(&items)
            .interact()?;

        Ok(options[selection])
    }
}

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
        let option = AuthOptionSelector::select()?;

        match option {
            AuthOption::SignUp => {
                // Sign Up
                let sign_up_cmd = JoinVaultCommand::new(self.base.db_name.clone());
                sign_up_cmd.execute().await?
            }
            AuthOption::AcceptJoinRequest => {
                // Accept Join Request
                let device_id = Input::<String>::new()
                    .with_prompt("Enter device ID")
                    .interact()?;

                let accept_cmd =
                    AcceptJoinRequestCommand::new(self.base.db_name.clone(), device_id);
                accept_cmd.execute().await?
            }
            AuthOption::AcceptAllJoinRequests => {
                // Accept All Join Requests
                let accept_all_cmd = AcceptAllJoinRequestsCommand::new(self.base.db_name.clone());
                accept_all_cmd.execute().await?
            }
            AuthOption::Back => {
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
    fn test_auth_option_order_matches_selection_indices() {
        // Collect all AuthOption variants in order
        let options: Vec<AuthOption> = AuthOption::iter().collect();

        // Verify the order matches expected indices
        assert_eq!(options.len(), 4);
        assert!(matches!(options[0], AuthOption::SignUp));
        assert!(matches!(options[1], AuthOption::AcceptJoinRequest));
        assert!(matches!(options[2], AuthOption::AcceptAllJoinRequests));
        assert!(matches!(options[3], AuthOption::Back));
    }

    #[test]
    fn test_auth_option_display_strings() {
        // Verify the Display implementation produces the correct strings
        assert_eq!(
            AuthOption::SignUp.to_string(),
            "Sign Up (Create/Join vault)"
        );
        assert_eq!(
            AuthOption::AcceptJoinRequest.to_string(),
            "Accept Join Request"
        );
        assert_eq!(
            AuthOption::AcceptAllJoinRequests.to_string(),
            "Accept All Join Requests"
        );
        assert_eq!(AuthOption::Back.to_string(), "Back to Main Menu");
    }
}
