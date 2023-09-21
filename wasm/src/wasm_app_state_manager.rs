use std::sync::Arc;

use async_trait::async_trait;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use meta_secret_core::models::application_state::ApplicationState;
use meta_secret_core::models::MetaPasswordId;
use meta_secret_core::node::app::app_state_manager::{
    ApplicationStateManager, ApplicationStateManagerConfigurator, JsAppStateManager,
};
use meta_secret_core::node::app::meta_app::messaging::{
    ClusterDistributionRequest, GenericAppStateRequest, RecoveryRequest, SignUpRequest,
};
use meta_secret_core::node::logger::LoggerId;

use crate::utils::WasmTaskRunner;
use crate::wasm_repo::{WasmMetaLogger, WasmRepo};
use crate::{configure, info, JsAppState};

pub struct WasmJsAppStateManager {}

#[async_trait(? Send)]
impl JsAppStateManager for WasmJsAppStateManager {
    async fn update_js_state(&self, _new_state: ApplicationState) {
        info("Update js state!!!");
    }
}

pub struct JsJsAppStateManager {
    js_app_state: JsAppState,
}

#[async_trait(? Send)]
impl JsAppStateManager for JsJsAppStateManager {
    async fn update_js_state(&self, new_state: ApplicationState) {
        let new_state_js = serde_wasm_bindgen::to_value(&new_state).unwrap();
        self.js_app_state.updateJsState(new_state_js).await;
    }
}

#[wasm_bindgen]
pub struct WasmApplicationStateManager {
    app_manager: GenericApplicationStateManager,
}

pub enum GenericApplicationStateManager {
    Wasm {
        app_state_manager: ApplicationStateManager<WasmRepo, WasmMetaLogger, JsJsAppStateManager>,
    },
    InMem {
        app_state_manager: ApplicationStateManager<WasmRepo, WasmMetaLogger, WasmJsAppStateManager>,
    },
}

#[wasm_bindgen]
impl WasmApplicationStateManager {
    pub async fn init_in_mem() -> WasmApplicationStateManager {
        info("Init Wasm state manager");

        configure();

        let cfg = ApplicationStateManagerConfigurator {
            client_repo: Arc::new(WasmRepo::default()),
            server_repo: Arc::new(WasmRepo::server()),
            device_repo: Arc::new(WasmRepo::virtual_device()),

            client_logger: Arc::new(WasmMetaLogger {
                id: LoggerId::Client,
            }),
            server_logger: Arc::new(WasmMetaLogger {
                id: LoggerId::Server,
            }),
            device_logger: Arc::new(WasmMetaLogger { id: LoggerId::Vd1 }),

            js_app_state: Arc::new(WasmJsAppStateManager {}),
            vd_js_app_state: Arc::new(WasmJsAppStateManager {}),

            task_runner: Arc::new(WasmTaskRunner {}),
        };

        let app_state_manager = ApplicationStateManager::init(cfg).await;
        WasmApplicationStateManager {
            app_manager: GenericApplicationStateManager::InMem { app_state_manager },
        }
    }

    pub async fn init_wasm(js_app_state: JsAppState) -> WasmApplicationStateManager {
        info("Init Wasm state manager");

        configure();

        let cfg = ApplicationStateManagerConfigurator {
            client_repo: Arc::new(WasmRepo::default()),
            server_repo: Arc::new(WasmRepo::server()),
            device_repo: Arc::new(WasmRepo::virtual_device()),

            client_logger: Arc::new(WasmMetaLogger {
                id: LoggerId::Client,
            }),
            server_logger: Arc::new(WasmMetaLogger {
                id: LoggerId::Server,
            }),
            device_logger: Arc::new(WasmMetaLogger { id: LoggerId::Vd1 }),

            js_app_state: Arc::new(JsJsAppStateManager { js_app_state }),
            vd_js_app_state: Arc::new(JsJsAppStateManager {
                js_app_state: JsAppState::new(),
            }),

            task_runner: Arc::new(WasmTaskRunner {}),
        };

        let app_state_manager = ApplicationStateManager::init(cfg).await;
        WasmApplicationStateManager {
            app_manager: GenericApplicationStateManager::Wasm { app_state_manager },
        }
    }

    pub async fn sign_up(&self, vault_name: &str, device_name: &str) {
        let request = GenericAppStateRequest::SignUp(SignUpRequest {
            vault_name: vault_name.to_string(),
            device_name: device_name.to_string(),
        });

        match &self.app_manager {
            GenericApplicationStateManager::Wasm { app_state_manager } => {
                app_state_manager
                    .meta_client_service
                    .send_request(request)
                    .await
            }
            GenericApplicationStateManager::InMem { app_state_manager } => {
                app_state_manager
                    .meta_client_service
                    .send_request(request)
                    .await
            }
        }
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
        }
    }

    pub async fn recover_js(&self, meta_pass_id_js: JsValue) {
        let meta_pass_id: MetaPasswordId = serde_wasm_bindgen::from_value(meta_pass_id_js).unwrap();

        let request = GenericAppStateRequest::Recover(RecoveryRequest { meta_pass_id });

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
