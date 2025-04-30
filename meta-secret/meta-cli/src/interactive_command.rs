use crate::auth::interactive_command::AuthInteractiveCommand;
use crate::base_command::BaseCommand;
use crate::cli_format::CliOutputFormat;
use crate::info::info_command::InfoCommand;
use crate::init::interactive_command::InitInteractiveCommand;
use crate::secret::interactive_command::SecretInteractiveCommand;
use anyhow::Result;
use dialoguer::{Select, theme::ColorfulTheme};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};

#[derive(Debug, Clone, Copy, Display, EnumIter)]
pub enum Category {
    #[strum(to_string = "Initialize")]
    Initialize,
    #[strum(to_string = "Authentication")]
    Authentication,
    #[strum(to_string = "Secret Management")]
    SecretManagement,
    #[strum(to_string = "Info")]
    ShowDeviceInfo,
    #[strum(to_string = "Exit")]
    Exit,
}

#[derive(Debug, Clone, Copy, Display, EnumIter)]
pub enum InfoSubCategory {
    #[strum(to_string = "Full Information")]
    Default,
    #[strum(to_string = "Recovery Claims")]
    RecoveryClaims,
    #[strum(to_string = "Secrets")]
    Secrets,
    #[strum(to_string = "Vault Events")]
    VaultEvents,
    #[strum(to_string = "Back")]
    Back,
}

pub struct CategorySelector;

impl CategorySelector {
    pub fn select() -> Result<Category> {
        let categories: Vec<Category> = Category::iter().collect();
        let items: Vec<String> = categories.iter().map(|c| c.to_string()).collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select category")
            .default(0)
            .items(&items)
            .interact()?;

        Ok(categories[selection])
    }
}

pub struct InfoSubCategorySelector;

impl InfoSubCategorySelector {
    pub fn select() -> Result<InfoSubCategory> {
        let categories: Vec<InfoSubCategory> = InfoSubCategory::iter().collect();
        let items: Vec<String> = categories.iter().map(|c| c.to_string()).collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select info category")
            .default(0)
            .items(&items)
            .interact()?;

        Ok(categories[selection])
    }
}

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
        loop {
            let category = CategorySelector::select()?;

            match category {
                Category::Initialize => {
                    let init_cmd = InitInteractiveCommand::new(self.base.db_name.clone());
                    init_cmd.execute().await?;
                }
                Category::Authentication => {
                    let auth_cmd = AuthInteractiveCommand::new(self.base.db_name.clone());
                    auth_cmd.execute().await?;
                }
                Category::SecretManagement => {
                    let secret_cmd = SecretInteractiveCommand::new(self.base.db_name.clone());
                    secret_cmd.execute().await?;
                }
                Category::ShowDeviceInfo => {
                    self.handle_info_commands().await?;
                }
                Category::Exit => {
                    println!("Exiting meta-cli");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_info_commands(&self) -> Result<()> {
        loop {
            let info_subcategory = InfoSubCategorySelector::select()?;
            
            if matches!(info_subcategory, InfoSubCategory::Back) {
                break;
            }
            
            let info_cmd = InfoCommand::new(self.base.db_name.clone(), CliOutputFormat::default());
            
            match info_subcategory {
                InfoSubCategory::Default => {
                    info_cmd.execute().await?;
                }
                InfoSubCategory::RecoveryClaims => {
                    info_cmd.show_recovery_claims().await?;
                }
                InfoSubCategory::Secrets => {
                    info_cmd.show_secrets().await?;
                }
                InfoSubCategory::VaultEvents => {
                    info_cmd.show_vault_events().await?;
                }
                InfoSubCategory::Back => unreachable!(), // Already handled above
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_order_matches_selection_indices() {
        // Collect all Category variants in order
        let categories: Vec<Category> = Category::iter().collect();

        // Verify the order matches expected indices
        assert_eq!(categories.len(), 5);
        assert!(matches!(categories[0], Category::Initialize));
        assert!(matches!(categories[1], Category::Authentication));
        assert!(matches!(categories[2], Category::SecretManagement));
        assert!(matches!(categories[3], Category::ShowDeviceInfo));
        assert!(matches!(categories[4], Category::Exit));
    }

    #[test]
    fn test_category_display_strings() {
        // Verify the Display implementation produces the correct strings
        assert_eq!(Category::Initialize.to_string(), "Initialize");
        assert_eq!(Category::Authentication.to_string(), "Authentication");
        assert_eq!(Category::SecretManagement.to_string(), "Secret Management");
        assert_eq!(Category::ShowDeviceInfo.to_string(), "Show Device Info");
        assert_eq!(Category::Exit.to_string(), "Exit");
    }
    
    #[test]
    fn test_info_subcategory_order_matches_selection_indices() {
        // Collect all InfoSubCategory variants in order
        let subcategories: Vec<InfoSubCategory> = InfoSubCategory::iter().collect();

        // Verify the order matches expected indices
        assert_eq!(subcategories.len(), 5);
        assert!(matches!(subcategories[0], InfoSubCategory::Default));
        assert!(matches!(subcategories[1], InfoSubCategory::RecoveryClaims));
        assert!(matches!(subcategories[2], InfoSubCategory::Secrets));
        assert!(matches!(subcategories[3], InfoSubCategory::VaultEvents));
        assert!(matches!(subcategories[4], InfoSubCategory::Back));
    }
}
