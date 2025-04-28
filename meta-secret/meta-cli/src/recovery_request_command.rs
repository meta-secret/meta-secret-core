use crate::base_command::BaseCommand;
use anyhow::{Result, bail};
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::common::model::meta_pass::MetaPasswordId;

pub struct RecoveryRequestCommand {
    pub base: BaseCommand,
    pub pass_id: MetaPasswordId,
}

impl RecoveryRequestCommand {
    pub fn new(db_name: String, pass_name: String) -> Self {
        
        Self {
            base: BaseCommand::new(db_name),
            pass_id: MetaPasswordId::build(pass_name)
        }
    }

    pub async fn execute(self) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;

        // Check if user credentials exist to get the vault name
        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;
        let Some(user_creds) = maybe_user_creds else {
            bail!("User credentials not found. Please run `meta-secret init-user` first.");
        };

        // Create client service
        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        // Create recovery request with password ID
        let recovery_request = GenericAppStateRequest::Recover(self.pass_id.clone());

        client
            .handle_client_request(app_state, recovery_request)
            .await?;

        println!("Recovery request for '{:?}' submitted successfully", self.pass_id);
        println!("The secret will be recovered when enough shares are available");

        Ok(())
    }
}
