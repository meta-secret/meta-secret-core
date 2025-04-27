use std::sync::Arc;
use anyhow::{bail, Result};
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use crate::base_command::BaseCommand;

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
        
        // Check if user credentials exist to get the vault name
        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;
        let Some(user_creds) = maybe_user_creds else {
            bail!("User credentials not found. Please run `meta-secret init-user` first.");
        };
        
        let vault_name = user_creds.vault_name.clone();
        
        // Create client service
        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;
        
        // Create signup request with the vault name
        let sign_up_request = GenericAppStateRequest::SignUp(vault_name);
        client.handle_client_request(app_state, sign_up_request).await?;
        
        Ok(())
    }
}