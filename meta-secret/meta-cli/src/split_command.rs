use crate::base_command::BaseCommand;
use anyhow::{Result, bail};
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::common::model::meta_pass::PassInfo;
use secrecy::SecretString;

pub struct SplitCommand {
    pub base: BaseCommand,
    pub pass: SecretString,
    pub pass_name: String,
}

impl SplitCommand {
    pub fn new(db_name: String, pass: String, pass_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
            pass: SecretString::new(pass.into()),
            pass_name,
        }
    }

    pub async fn execute(&self) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;

        // Check if user credentials exist to get the vault name
        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;
        let Some(_) = maybe_user_creds else {
            bail!("User credentials not found. Please run `meta-secret init-user` first.");
        };

        // Create client service
        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        // Create cluster distribution request with password
        let pass_info = PassInfo::new(self.pass.clone(), self.pass_name.clone());
        let cluster_distribution_request = GenericAppStateRequest::ClusterDistribution(pass_info);

        client
            .handle_client_request(app_state, cluster_distribution_request)
            .await?;

        Ok(())
    }
}
