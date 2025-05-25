use std::sync::Arc;
use tracing::info;
use meta_db_sqlite::db::sqlite_migration::EmbeddedMigrationsTool;
use meta_db_sqlite::db::sqlite_store::SqlIteRepo;
use meta_secret_core::crypto::keys::TransportSk;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::ClaimId;
use meta_secret_core::node::common::model::user::common::UserData;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::common::model::WasmApplicationState;
use meta_secret_core::node::db::actions::sign_up::join::JoinActionUpdate;
use meta_secret_wasm::app_manager::ApplicationManager;
use meta_secret_wasm::configure;

pub struct AndroidApplicationManager {
    app_manager: ApplicationManager<SqlIteRepo, HttpSyncProtocol>,
}

impl AndroidApplicationManager {
    pub async fn init_ios(master_key: TransportSk) -> anyhow::Result<AndroidApplicationManager> {
        configure();

        info!("Init iOS state manager");

        let db_path = "meta-secret.db";
        let conn_url = format!("file:{}", db_path);

        let migration_tool = EmbeddedMigrationsTool {
            db_url: conn_url.clone(),
        };
        migration_tool.migrate();

        let repo = SqlIteRepo { conn_url };
        let client_repo = Arc::new(repo);

        let app_manager =
            ApplicationManager::<SqlIteRepo, HttpSyncProtocol>::init(client_repo, master_key)
                .await?;

        Ok(AndroidApplicationManager { app_manager })
    }

    pub async fn get_state(&self) -> WasmApplicationState {
        let app_state = self.app_manager.get_state().await;
        WasmApplicationState::from(app_state)
    }

    pub async fn generate_user_creds(&self, vault_name: String) {
        info!("Generate user credentials for vault: {}", vault_name);
        self.app_manager
            .generate_user_creds(VaultName::from(vault_name))
            .await;
    }

    pub async fn sign_up(&self) -> WasmApplicationState {
        let app_state = self.app_manager.sign_up().await.unwrap();
        WasmApplicationState::from(app_state)
    }

    pub async fn update_membership(&self, candidate: UserData, upd: JoinActionUpdate) {
        self.app_manager
            .update_membership(candidate, upd)
            .await
            .unwrap()
    }

    pub async fn cluster_distribution(&self, plain_pass_info: &PlainPassInfo) {
        self.app_manager
            .cluster_distribution(plain_pass_info.clone())
            .await;
    }

    pub async fn recover_js(&self, meta_pass_id: &MetaPasswordId) {
        self.app_manager.recover_js(meta_pass_id.clone()).await;
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