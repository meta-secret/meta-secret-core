use crate::base_command::BaseCommand;
use crate::cli_format::CliOutputFormat;
use anyhow::Result;
use meta_secret_core::node::common::model::ApplicationState;

// Base trait for info commands
pub trait InfoCommandTrait {
    async fn execute(&self) -> Result<()>;
}

// Base command with common functionality
pub struct InfoCommandBase {
    base: BaseCommand,
    output_format: CliOutputFormat,
}

impl InfoCommandBase {
    pub fn new(db_name: String, output_format: CliOutputFormat) -> Self {
        Self {
            base: BaseCommand::new(db_name),
            output_format,
        }
    }

    /// Helper method to initialize client and get application state
    pub async fn get_app_state(&self) -> Result<ApplicationState> {
        let db_context = self.base.open_existing_db().await?;
        let client = self.base.create_client_service(&db_context).await?;
        client.get_app_state().await
    }

    pub fn base(&self) -> &BaseCommand {
        &self.base
    }

    pub fn output_format(&self) -> CliOutputFormat {
        self.output_format
    }
} 