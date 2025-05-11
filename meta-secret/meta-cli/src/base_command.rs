use anyhow::{Result, bail};
use meta_db_redb::ReDbRepo;
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::app::meta_app::meta_client_service::{
    MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
};
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use std::path::Path;
use std::sync::Arc;

/// Container for database-related components
pub struct DbContext<Repo: KvLogEventRepo> {
    pub repo: Arc<Repo>,
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub p_creds: PersistentCredentials<Repo>,
}

pub struct BaseCommand {
    pub db_name: String,
    pub api_url: ApiUrl
}

impl BaseCommand {
    pub fn new(db_name: String) -> Self {
        Self { 
            db_name,
            api_url: ApiUrl::prod()
        }
    }

    /// Opens an existing database and returns a context with repo, persistent object and credentials
    pub async fn open_existing_db(&self) -> Result<DbContext<ReDbRepo>> {
        let db_path = Path::new(self.db_name.as_str());

        if !db_path.exists() {
            bail!("Database not found. Please run 'meta-secret init-device' command first.");
        }

        let repo = Arc::new(ReDbRepo::open(db_path)?);
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));
        let p_creds = PersistentCredentials {
            p_obj: p_obj.clone(),
        };

        Ok(DbContext {
            repo,
            p_obj,
            p_creds,
        })
    }

    /// Opens an existing database or creates a new one if it doesn't exist
    pub async fn open_or_create_db(&self) -> Result<DbContext<ReDbRepo>> {
        let db_path = Path::new(self.db_name.as_str());

        // Check if database exists and either open or create it
        let repo = if db_path.exists() {
            Arc::new(ReDbRepo::open(db_path)?)
        } else {
            Arc::new(ReDbRepo::new(db_path)?)
        };

        let p_obj = Arc::new(PersistentObject::new(repo.clone()));
        let p_creds = PersistentCredentials {
            p_obj: p_obj.clone(),
        };

        Ok(DbContext {
            repo,
            p_obj,
            p_creds,
        })
    }

    /// Common error message for credentials that already exist
    pub fn already_exists_error(entity: &str) -> String {
        let err_msg = "credentials already exist. Cannot initialize again.";
        let info_msg = "Use the 'info' command to view existing credentials.";
        format!("{} {} {}", entity, err_msg, info_msg)
    }

    /// Common error handling for missing device credentials
    pub async fn ensure_device_creds<Repo: KvLogEventRepo>(&self, db_context: &DbContext<Repo>) -> Result<()> {
        if db_context.p_creds.get_device_creds().await?.is_none() {
            bail!("Device credentials not found. Please run `meta-secret init-device` first.");
        }
        Ok(())
    }

    /// Common error handling for missing user credentials
    pub async fn ensure_user_creds<Repo: KvLogEventRepo>(&self, db_context: &DbContext<Repo>) -> Result<()> {
        if db_context.p_creds.get_user_creds().await?.is_none() {
            bail!("User credentials not found. Please run `meta-secret init-user` first.");
        }
        Ok(())
    }

    /// Helper method to create client, get app state, and handle a request
    pub async fn handle_client_request(
        &self,
        db_context: &DbContext<ReDbRepo>,
        request: GenericAppStateRequest,
    ) -> Result<()> {
        let client = self.create_client_service(db_context).await?;
        let app_state = client.get_app_state().await?;

        client.handle_client_request(app_state, request).await?;

        Ok(())
    }

    /// Creates a MetaClientService using the user credentials from the database
    pub async fn create_client_service(
        &self,
        db_context: &DbContext<ReDbRepo>,
    ) -> Result<MetaClientService<ReDbRepo, HttpSyncProtocol>> {
        // Get user credentials from the database
        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;
        let Some(user_creds) = maybe_user_creds else {
            bail!("User credentials not found. Please run `meta-secret init-user` first.");
        };

        let device_creds = Arc::new(user_creds.device_creds.clone());

        let sync_protocol = HttpSyncProtocol {
            api_url: self.api_url,
        };

        let sync_gateway = Arc::new(SyncGateway {
            id: "meta-cli".to_string(),
            p_obj: db_context.p_obj.clone(),
            sync: Arc::new(sync_protocol),
            device_creds: device_creds.clone(),
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());

        Ok(MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer {
                dt: MpscDataTransfer::new(),
            }),
            sync_gateway,
            state_provider,
            p_obj: db_context.p_obj.clone(),
            device_creds,
        })
    }
}
