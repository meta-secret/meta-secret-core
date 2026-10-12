use crate::base_command::BaseCommand;
use anyhow::Result;
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
            pass_id: MetaPasswordId::build(pass_name),
        }
    }

    pub async fn execute(self) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;

        // Ensure user credentials exist
        self.base.ensure_user_creds(&db_context).await?;

        // Create recovery request with password ID and handle it
        let client = self.base.create_client_service(&db_context).await?;
        let state = client.get_app_state().await?;
        let pass_id = client.resolve_secret(&self.pass_id.name).await?;
        client
            .handle_client_request(state, GenericAppStateRequest::Recover(pass_id))
            .await?;

        println!(
            "Recovery request for '{:?}' submitted successfully",
            self.pass_id
        );
        println!("The secret will be recovered when enough shares are available");

        Ok(())
    }
}
