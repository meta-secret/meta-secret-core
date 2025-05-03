use std::sync::Arc;

use tracing::info;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::app_manager::ApplicationManager;
use crate::configure;
use crate::wasm_repo::{WasmRepo, WasmSyncProtocol};
use meta_secret_core::node::app::app_state_update_manager::ApplicationManagerConfigurator;
use meta_secret_core::node::common::model::WasmApplicationState;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::vault::vault::VaultName;

#[wasm_bindgen]
pub struct WasmApplicationManager {
    app_manager: ApplicationManager<WasmRepo, WasmSyncProtocol<WasmRepo>>,
}

#[wasm_bindgen]
impl WasmApplicationManager {
    pub async fn init_wasm() -> WasmApplicationManager {
        configure();

        info!("Init Wasm state manager");

        let cfg = ApplicationManagerConfigurator {
            client_repo: Arc::new(WasmRepo::default().await),
            server_repo: Arc::new(WasmRepo::server().await),
            device_repo: Arc::new(WasmRepo::virtual_device().await),
        };

        let app_manager = ApplicationManager::<WasmRepo, WasmSyncProtocol<WasmRepo>>::init(cfg)
            .await
            .expect("Application state manager must be initialized");

        WasmApplicationManager { app_manager }
    }

    pub async fn get_state(&self) -> WasmApplicationState {
        let app_state = self.app_manager.get_state().await;
        WasmApplicationState::from(app_state)
    }

    pub async fn generate_user_creds(&self, vault_name: String) {
        self.app_manager
            .generate_user_creds(VaultName::from(vault_name))
            .await;
    }

    pub async fn sign_up(&self) {
        self.app_manager.sign_up().await.unwrap();
    }

    pub async fn cluster_distribution(&self, plain_pass_info: PlainPassInfo) {
        self.app_manager.cluster_distribution(plain_pass_info).await;
    }

    pub async fn recover_js(&self, meta_pass_id: MetaPasswordId) {
        self.app_manager.recover_js(meta_pass_id).await;
    }
}
