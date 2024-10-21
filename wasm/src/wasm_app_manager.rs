use std::sync::Arc;

use tracing::info;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::app_manager::{ApplicationManager, WasmApplicationState};
use crate::configure;
use crate::wasm_repo::WasmRepo;
use meta_secret_core::node::app::app_state_update_manager::ApplicationManagerConfigurator;
use meta_secret_core::node::app::meta_app::messaging::{
    ClusterDistributionRequest, GenericAppStateRequest,
};
use meta_secret_core::node::common::model::vault::VaultName;

#[wasm_bindgen]
pub struct WasmApplicationManager {
    app_manager: GenericApplicationManager,
}

pub enum GenericApplicationManager {
    Wasm {
        app_manager: ApplicationManager<WasmRepo>,
    },
    InMem {
        app_manager: ApplicationManager<WasmRepo>,
    },
}

impl GenericApplicationManager {
    pub fn get_app_manager(&self) -> &ApplicationManager<WasmRepo> {
        match self {
            GenericApplicationManager::Wasm { app_manager } => app_manager,
            GenericApplicationManager::InMem { app_manager } => app_manager,
        }
    }
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

        WasmApplicationManager {
            app_manager: GenericApplicationManager::InMem { app_manager },
        }
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

        WasmApplicationManager {
            app_manager: GenericApplicationManager::Wasm { app_manager },
        }
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

        match &self.app_manager {
            GenericApplicationManager::Wasm { app_manager } => {
                app_manager.meta_client_service.send_request(sign_up).await;
            }
            GenericApplicationManager::InMem { app_manager } => {
                app_manager.meta_client_service.send_request(sign_up).await;
            }
        };
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        let request = GenericAppStateRequest::ClusterDistribution(ClusterDistributionRequest {
            pass_id: pass_id.to_string(),
            pass: pass.to_string(),
        });

        match &self.app_manager {
            GenericApplicationManager::Wasm { app_manager } => {
                app_manager.meta_client_service.send_request(request).await;
            }
            GenericApplicationManager::InMem { app_manager } => {
                app_manager.meta_client_service.send_request(request).await;
            }
        };
    }

    pub async fn recover_js(&self, meta_pass_id_js: JsValue) {
        let meta_pass_id = serde_wasm_bindgen::from_value(meta_pass_id_js).unwrap();

        let request = GenericAppStateRequest::Recover(meta_pass_id);

        match &self.app_manager {
            GenericApplicationManager::Wasm { app_manager } => {
                app_manager.meta_client_service.send_request(request).await;
            }
            GenericApplicationManager::InMem { app_manager } => {
                app_manager.meta_client_service.send_request(request).await;
            }
        }
    }
}
