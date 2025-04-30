use crate::base_command::BaseCommand;
use crate::cli_format::CliOutputFormat;
use crate::info::default_info_command::DefaultInfoCommand;
use crate::info::recovery_claims_command::RecoveryClaimsInfoCommand;
use crate::info::secrets_command::SecretsInfoCommand;
use crate::info::vault_events_command::VaultEventsInfoCommand;
use anyhow::Result;
use dialoguer::{Select, theme::ColorfulTheme};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};
use crate::info::info_command_base::InfoCommandTrait;

#[derive(Debug, Clone, Copy, Display, EnumIter)]
pub enum InfoOption {
    #[strum(to_string = "Full Information")]
    Default,
    #[strum(to_string = "Recovery Claims")]
    RecoveryClaims,
    #[strum(to_string = "Secrets")]
    Secrets,
    #[strum(to_string = "Vault Events")]
    VaultEvents,
    #[strum(to_string = "Back to Main Menu")]
    Back,
}

pub struct InfoOptionSelector;

impl InfoOptionSelector {
    pub fn select() -> Result<InfoOption> {
        let options: Vec<InfoOption> = InfoOption::iter().collect();
        let items: Vec<String> = options.iter().map(|o| o.to_string()).collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select info category")
            .default(0)
            .items(&items)
            .interact()?;

        Ok(options[selection])
    }
}

pub struct InfoInteractiveCommand {
    base: BaseCommand,
}

impl InfoInteractiveCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    pub async fn execute(&self) -> Result<()> {
        loop {
            let option = InfoOptionSelector::select()?;
            
            if matches!(option, InfoOption::Back) {
                break;
            }
            
            let output_format = CliOutputFormat::default();
            
            match option {
                InfoOption::Default => {
                    let cmd = DefaultInfoCommand::new(self.base.db_name.clone(), output_format);
                    cmd.execute().await?;
                },
                InfoOption::RecoveryClaims => {
                    let cmd = RecoveryClaimsInfoCommand::new(self.base.db_name.clone(), output_format);
                    cmd.execute().await?;
                },
                InfoOption::Secrets => {
                    let cmd = SecretsInfoCommand::new(self.base.db_name.clone(), output_format);
                    cmd.execute().await?;
                },
                InfoOption::VaultEvents => {
                    let cmd = VaultEventsInfoCommand::new(self.base.db_name.clone(), output_format);
                    cmd.execute().await?;
                },
                InfoOption::Back => unreachable!(), // Already handled above
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_info_option_order_matches_selection_indices() {
        // Collect all InfoOption variants in order
        let options: Vec<InfoOption> = InfoOption::iter().collect();

        // Verify the order matches expected indices
        assert_eq!(options.len(), 5);
        assert!(matches!(options[0], InfoOption::Default));
        assert!(matches!(options[1], InfoOption::RecoveryClaims));
        assert!(matches!(options[2], InfoOption::Secrets));
        assert!(matches!(options[3], InfoOption::VaultEvents));
        assert!(matches!(options[4], InfoOption::Back));
    }

    #[test]
    fn test_info_option_display_strings() {
        // Verify the Display implementation produces the correct strings
        assert_eq!(InfoOption::Default.to_string(), "Full Information");
        assert_eq!(InfoOption::RecoveryClaims.to_string(), "Recovery Claims");
        assert_eq!(InfoOption::Secrets.to_string(), "Secrets");
        assert_eq!(InfoOption::VaultEvents.to_string(), "Vault Events");
        assert_eq!(InfoOption::Back.to_string(), "Back to Main Menu");
    }
} 