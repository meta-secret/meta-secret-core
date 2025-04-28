use crate::base_command::BaseCommand;
use anyhow::{Result, bail};
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::common::model::meta_pass::{PlainPassInfo};

pub struct SplitCommand {
    pub base: BaseCommand,
}

impl SplitCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    pub async fn execute(self, pass: PlainPassInfo) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;

        // Check if user credentials exist to get the vault name
        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;
        let Some(_) = maybe_user_creds else {
            bail!("User credentials not found. Please run `meta-secret init-user` first.");
        };

        // Create client service
        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        let request = GenericAppStateRequest::ClusterDistribution(pass);

        client
            .handle_client_request(app_state, request)
            .await?;

        Ok(())
    }
}
