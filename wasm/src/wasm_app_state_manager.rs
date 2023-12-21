use std::sync::Arc;

use async_trait::async_trait;
use tracing::info;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use meta_secret_core::node::app::app_state_update_manager::{
    ApplicationStateManagerConfigurator, JsAppStateManager, NoOpJsAppStateManager,
};
use meta_secret_core::node::app::meta_app::messaging::{
    ClusterDistributionRequest, GenericAppStateRequest
};
use meta_secret_core::node::common::model::ApplicationState;
use crate::app_state_manager::ApplicationStateManager;
use crate::{configure, updateJsState};
use crate::wasm_repo::WasmRepo;

pub struct JsJsAppStateManager {}

#[async_trait(? Send)]
impl JsAppStateManager for JsJsAppStateManager {
    async fn update_js_state(&self, new_state: ApplicationState) {
        let new_state_js = serde_wasm_bindgen::to_value(&new_state).unwrap();
        updateJsState(new_state_js).await;
    }
}

#[wasm_bindgen]
pub struct WasmApplicationStateManager {
    app_manager: GenericApplicationStateManager,
}

pub enum GenericApplicationStateManager {
    Wasm {
        app_state_manager: ApplicationStateManager<WasmRepo, JsJsAppStateManager>,
    },
    InMem {
        app_state_manager: ApplicationStateManager<WasmRepo, NoOpJsAppStateManager>,
    },
}

#[wasm_bindgen]
impl WasmApplicationStateManager {
    pub async fn init_in_mem() -> WasmApplicationStateManager {
        configure();

        info!("Init Wasm state manager");

        let cfg = ApplicationStateManagerConfigurator {
            client_repo: Arc::new(WasmRepo::default()),
            server_repo: Arc::new(WasmRepo::server()),
            device_repo: Arc::new(WasmRepo::virtual_device()),

            js_app_state: Arc::new(NoOpJsAppStateManager {}),
            vd_js_app_state: Arc::new(NoOpJsAppStateManager {}),
        };

        let app_state_manager = ApplicationStateManager::init(cfg)
            .await
            .expect("Application state manager must be initialized");

        WasmApplicationStateManager {
            app_manager: GenericApplicationStateManager::InMem { app_state_manager },
        }
    }

    pub async fn init_wasm() -> WasmApplicationStateManager {
        configure();

        info!("Init Wasm state manager");

        let cfg = ApplicationStateManagerConfigurator {
            client_repo: Arc::new(WasmRepo::default()),
            server_repo: Arc::new(WasmRepo::server()),
            device_repo: Arc::new(WasmRepo::virtual_device()),
            js_app_state: Arc::new(JsJsAppStateManager {}),
            vd_js_app_state: Arc::new(JsJsAppStateManager {}),
        };

        let app_state_manager = ApplicationStateManager::init(cfg)
            .await
            .expect("Application state manager must be initialized");

        WasmApplicationStateManager {
            app_manager: GenericApplicationStateManager::Wasm { app_state_manager },
        }
    }

    pub async fn sign_up(&self) {
        info!("Sign Up");

        let sign_up = GenericAppStateRequest::SignUp;

        match &self.app_manager {
            GenericApplicationStateManager::Wasm { app_state_manager } => {
                app_state_manager
                    .meta_client_service
                    .send_request(sign_up)
                    .await;
            }
            GenericApplicationStateManager::InMem { app_state_manager } => {
                app_state_manager
                    .meta_client_service
                    .send_request(sign_up)
                    .await;
            }
        };
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        let request = GenericAppStateRequest::ClusterDistribution(ClusterDistributionRequest {
            pass_id: pass_id.to_string(),
            pass: pass.to_string(),
        });

        match &self.app_manager {
            GenericApplicationStateManager::Wasm { app_state_manager } => {
                app_state_manager
                    .meta_client_service
                    .send_request(request)
                    .await;
            }
            GenericApplicationStateManager::InMem { app_state_manager } => {
                app_state_manager
                    .meta_client_service
                    .send_request(request)
                    .await;
            }
        };
    }

    pub async fn recover_js(&self, meta_pass_id_js: JsValue) {
        let meta_pass_id = serde_wasm_bindgen::from_value(meta_pass_id_js).unwrap();

        let request = GenericAppStateRequest::Recover(meta_pass_id);

        match &self.app_manager {
            GenericApplicationStateManager::Wasm { app_state_manager } => {
                app_state_manager
                    .meta_client_service
                    .send_request(request)
                    .await;
            }
            GenericApplicationStateManager::InMem { app_state_manager } => {
                app_state_manager
                    .meta_client_service
                    .send_request(request)
                    .await;
            }
        }
    }
}
