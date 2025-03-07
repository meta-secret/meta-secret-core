use std::sync::Arc;

use tracing::info;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::app_manager::ApplicationManager;
use crate::configure;
use crate::wasm_repo::WasmRepo;
use meta_secret_core::node::app::app_state_update_manager::ApplicationManagerConfigurator;
use meta_secret_core::node::app::meta_app::messaging::{
    ClusterDistributionRequest, GenericAppStateRequest,
};
use meta_secret_core::node::common::model::meta_pass::MetaPasswordId;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::common::model::WasmApplicationState;

#[wasm_bindgen]
pub struct WasmApplicationManager {
    app_manager: ApplicationManager<WasmRepo>,
}

#[wasm_bindgen]
impl WasmApplicationManager {
    pub async fn init_in_mem() -> WasmApplicationManager {
        configure();

        info!("Init Wasm state manager");

        let cfg = ApplicationManagerConfigurator {
            client_repo: Arc::new(WasmRepo::default().await),
            server_repo: Arc::new(WasmRepo::server().await),
            device_repo: Arc::new(WasmRepo::virtual_device().await),
        };

        let app_manager = ApplicationManager::init(cfg)
            .await
            .expect("Application manager must be initialized");

        WasmApplicationManager { app_manager }
    }

    pub async fn init_wasm() -> WasmApplicationManager {
        configure();

        info!("Init Wasm state manager");

        let cfg = ApplicationManagerConfigurator {
            client_repo: Arc::new(WasmRepo::default().await),
            server_repo: Arc::new(WasmRepo::server().await),
            device_repo: Arc::new(WasmRepo::virtual_device().await),
        };

        let app_manager = ApplicationManager::init(cfg)
            .await
            .expect("Application state manager must be initialized");

        WasmApplicationManager { app_manager }
    }

    pub async fn get_state(&self) -> WasmApplicationState {
        let app_state = self
            .app_manager
            .get_app_manager()
            .meta_client_service
            .state_provider
            .get()
            .await;
        WasmApplicationState::from(app_state)
    }

    pub async fn sign_up(&self, vault_name: String) {
        info!("Sign Up");
        let sign_up = GenericAppStateRequest::SignUp(VaultName::from(vault_name.as_str()));
        self.app_manager.meta_client_service.send_request(sign_up).await;
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        let request = GenericAppStateRequest::ClusterDistribution(ClusterDistributionRequest {
            pass_id: MetaPasswordId::build(pass_id),
            pass: pass.to_string(),
        });

        self.app_manager.meta_client_service.send_request(request).await;
    }

    pub async fn recover_js(&self, meta_pass_id: MetaPasswordId) {
        let request = GenericAppStateRequest::Recover(meta_pass_id);
        self.app_manager.meta_client_service.send_request(request).await;
    }
}
