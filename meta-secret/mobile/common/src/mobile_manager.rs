use std::sync::Arc;
use tracing::info;
use meta_db_sqlite::db::sqlite_migration::EmbeddedMigrationsTool;
use meta_db_sqlite::db::sqlite_store::SqlIteRepo;
use meta_secret_core::crypto::keys::TransportSk;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::ClaimId;
use meta_secret_core::node::common::model::{ApplicationState, WasmApplicationState};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::future::Future;
use std::path::PathBuf;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use crate::app_manager::ApplicationManager;

static GLOBAL_APP_MANAGER: Lazy<Mutex<Option<Arc<MobileApplicationManager>>>> =
    Lazy::new(|| Mutex::new(None));

static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
});

pub struct MobileApplicationManager {
    app_manager: ApplicationManager<SqlIteRepo, HttpSyncProtocol>,
}

impl MobileApplicationManager {
    pub fn sync_wrapper<F: Future>(future: F) -> F::Output {
        RUNTIME.block_on(future)
    }

    pub fn set_global_instance(manager: Arc<MobileApplicationManager>) {
        let mut global = GLOBAL_APP_MANAGER.lock().unwrap();
        *global = Some(manager);
    }

    pub fn get_global_instance() -> Option<Arc<MobileApplicationManager>> {
        let global = GLOBAL_APP_MANAGER.lock().unwrap();
        global.clone()
    }

    pub fn is_global_initialized() -> bool {
        let global = GLOBAL_APP_MANAGER.lock().unwrap();
        global.is_some()
    }
    
    pub async fn init_ios(master_key: TransportSk) -> anyhow::Result<MobileApplicationManager> {
        let home_dir = std::env::var("HOME").expect("Unable to get HOME directory");
        let db_path = PathBuf::from(home_dir)
            .join("Documents")
            .join("meta-secret.db")
            .to_string_lossy()
            .to_string();
        info!("iOS database path: {}", db_path);
        
        Self::init(master_key, &db_path).await
    }
    
    pub async fn init_android(master_key: TransportSk) -> anyhow::Result<MobileApplicationManager> {
        let db_path = "/data/data/com.metasecret.core/databases/meta-secret.db";
        Self::init(master_key, db_path).await
    }

    pub async fn get_state(&self) -> WasmApplicationState {
        let app_state = self.app_manager.get_state().await;
        WasmApplicationState::from(app_state)
    }
    
    pub async fn generate_user_creds(&self, vault_name: VaultName) -> anyhow::Result<ApplicationState> {
        info!("Generate user credentials for vault: {}", vault_name);
        let app_state = self.app_manager
            .generate_user_creds(vault_name)
            .await?;
        Ok(app_state)
    }

    pub async fn sign_up(&self) -> WasmApplicationState {
        let app_state = self.app_manager.sign_up().await.unwrap();
        WasmApplicationState::from(app_state)
    }

    pub async fn cluster_distribution(&self, plain_pass_info: &PlainPassInfo) {
        self.app_manager
            .cluster_distribution(plain_pass_info.clone())
            .await;
    }

    pub async fn recover(&self, meta_pass_id: &MetaPasswordId) {
        self.app_manager.recover_js(meta_pass_id.clone()).await;
    }

    pub async fn accept_recover(&self, claim_id: ClaimId) {
        match self.app_manager.accept_recover(claim_id).await {
            Ok(res) => {res}
            Err(e) => {panic!("{}", e)}
        };
    }

    pub async fn show_recovered(&self, pass_id: &MetaPasswordId) -> String {
        info!("Show recovered pass id: {:?}", pass_id);
        self.app_manager
            .show_recovered(pass_id.clone())
            .await
            .unwrap()
            .text
    }

    pub async fn clean_up_database(&self) {
        self.app_manager.clean_up_database().await
    }

    pub async fn find_claim_by_pass_id(&self, pass_id: &MetaPasswordId) -> Option<ClaimId> {
        self.app_manager.find_claim_by_pass_id(pass_id).await
    }
} 

impl MobileApplicationManager {
    async fn init(master_key: TransportSk, db_path: &str) -> anyhow::Result<MobileApplicationManager> {
        info!("Init mobile state manager");
        info!("Using database path: {}", db_path);
        
        match master_key.pk() {
            Ok(pk) => info!("Master key valid. Public key available"),
            Err(e) => {
                info!("Invalid master key provided: {}", e);
                return Err(anyhow::anyhow!("Invalid master key: {}", e));
            }
        }

        if let Some(parent_dir) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent_dir)
                .map_err(|e| {
                    info!("Failed to create database directory: {}", e);
                    anyhow::anyhow!("Failed to create database directory: {}", e)
                })?;
        }

        let conn_url = format!("file:{}", db_path);

        let migration_tool = EmbeddedMigrationsTool {
            db_url: conn_url.clone(),
        };
        
        match std::panic::catch_unwind(|| {
            migration_tool.migrate();
        }) {
            Ok(_) => info!("Database migration successful"),
            Err(e) => {
                let err_msg = if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else {
                    "Unknown error during migration".to_string()
                };
                info!("Migration failed: {}", err_msg);
            }
        };

        let repo = SqlIteRepo { conn_url };
        let client_repo = Arc::new(repo);

        let app_manager =
            ApplicationManager::<SqlIteRepo, HttpSyncProtocol>::init(client_repo, master_key)
                .await?;

        Ok(MobileApplicationManager { app_manager })
    }
}