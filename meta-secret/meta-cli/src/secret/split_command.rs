use crate::base_command::BaseCommand;
use anyhow::Result;
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::common::model::meta_pass::PlainPassInfo;

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

        // Ensure user credentials exist
        self.base.ensure_user_creds(&db_context).await?;

        // Handle cluster distribution request
        let request = GenericAppStateRequest::ClusterDistribution(pass.clone());
        self.base
            .handle_client_request(&db_context, request)
            .await?;

        println!("Secret '{}' has been split successfully", pass.pass_id.name);
        Ok(())
    }
}
