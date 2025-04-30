use crate::base_command::BaseCommand;
use anyhow::Result;
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;

pub struct JoinVaultCommand {
    pub base: BaseCommand,
}

impl JoinVaultCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    pub async fn execute(&self) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;

        // Ensure user credentials exist
        self.base.ensure_user_creds(&db_context).await?;

        // Get vault name from credentials
        let user_creds = db_context.p_creds.get_user_creds().await?.unwrap();
        let vault_name = user_creds.vault_name.clone();

        // Create signup request with the vault name and handle it
        let sign_up_request = GenericAppStateRequest::SignUp(vault_name);
        self.base
            .handle_client_request(&db_context, sign_up_request)
            .await?;

        println!("Join vault request submitted successfully");
        Ok(())
    }
}
