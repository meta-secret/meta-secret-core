use std::sync::Arc;
use anyhow::{bail, Result};
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::app::meta_app::meta_client_service::{MetaClientDataTransfer, MetaClientService, MetaClientStateProvider};
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use crate::base_command::BaseCommand;

pub struct JoinVaultCommand {
    pub base: BaseCommand,
}

impl JoinVaultCommand {
    
    pub async fn execute(&self) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;

        // Check if user credentials already exist
        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;
        let Some(user_creds) = maybe_user_creds else {
            bail!("User credentials not found. Please run `meta-secret init-user` first.");
        };

        let sync_protocol = HttpSyncProtocol {
            api_url: ApiUrl::prod(),
        };

        let device_creds = Arc::new(user_creds.device_creds.clone());
        let sync_gateway = Arc::new(SyncGateway {
            id: "meta-cli".to_string(),
            p_obj: db_context.p_obj.clone(),
            sync: Arc::new(sync_protocol),
            device_creds: device_creds.clone(),
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());

        let client = MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer {
                dt: MpscDataTransfer::new(),
            }),
            sync_gateway,
            state_provider,
            p_obj: db_context.p_obj,
            device_creds,
        };

        let app_state = client.get_app_state().await?;
        let sign_up_request = GenericAppStateRequest::SignUp(user_creds.vault_name);
        
        client.handle_client_request(app_state, sign_up_request).await?;
        
        Ok(())
    }
}