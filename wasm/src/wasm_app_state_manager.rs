use wasm_bindgen_futures::spawn_local;
use meta_secret_core::node::app::app_state_manager::{ApplicationStateManager, ApplicationStateManagerApi, AppManagerAsyncRunner, StateUpdateManager};
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use crate::wasm_repo::{WasmMetaLogger, WasmRepo};
use async_trait::async_trait;
use wasm_bindgen::prelude::wasm_bindgen;
use crate::JsAppState;
use std::rc::Rc;
use wasm_bindgen::JsValue;
use meta_secret_core::models::application_state::ApplicationState;
use meta_secret_core::models::MetaPasswordId;
use meta_secret_core::node::logger::LoggerId;
use crate::wasm_server::WasmServer;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;

#[wasm_bindgen]
pub struct WasmApplicationStateManager {
    app_manager: ApplicationStateManager<WasmRepo, WasmMetaLogger, WasmStateUpdateManager>
}

#[async_trait(? Send)]
impl AppManagerAsyncRunner<WasmRepo, WasmMetaLogger> for WasmApplicationStateManager {
    async fn run(self) {
        self.init().await;
    }
}

#[wasm_bindgen]
impl WasmApplicationStateManager {
    pub async fn new(js_app_state: JsAppState) -> WasmApplicationStateManager {
        let client_repo = Rc::new(WasmRepo::default());
        let device_repo = Rc::new(WasmRepo::virtual_device());

        let client_logger = Rc::new(WasmMetaLogger {
            id: LoggerId::Client
        });

        let vd_logger = Rc::new(WasmMetaLogger {
            id: LoggerId::Vd1
        });

        let data_transfer = Rc::new(MpscDataTransfer::new());

        let wasm_server = Rc::new(WasmServer::new(data_transfer.clone()).await);

        let update_manager = Rc::new(WasmStateUpdateManager {
            js_app_state
        });

        let app_manager = ApplicationStateManager::new(
            client_repo, device_repo,
            client_logger, vd_logger,
            wasm_server.server.clone(),
            data_transfer,
            update_manager,
        );

        WasmApplicationStateManager { app_manager }
    }


    pub async fn init(mut self) {
        let server = self.app_manager.server.clone();
        spawn_local(async move {
            let _ = server.run().await;
        });

        let device_repo = self.app_manager.device_repo.clone();
        let data_transfer = self.app_manager.data_transfer.clone();
        let logger = self.app_manager.vd_logger.clone();
        spawn_local(async move {
            VirtualDevice::event_handler(
                device_repo,
                data_transfer,
                logger,
            ).await;
        });

        self.app_manager.setup_meta_client().await;
        let ctx = self.app_manager.meta_client.get_ctx();
        let repo = ctx.repo.clone();
        let data_transfer = self.app_manager.data_transfer.clone();

        let app_manager_rc = Rc::new(self.app_manager);
        let app_manager_rc_async = app_manager_rc.clone();

        spawn_local(async move {
            app_manager_rc_async.run_client_gateway(data_transfer, ctx, repo).await;
        });

        app_manager_rc.on_update().await;
    }
}

#[wasm_bindgen]
impl WasmApplicationStateManager {
    pub async fn sign_up(&mut self, vault_name: &str, device_name: &str) {
        self.app_manager.sign_up(vault_name, device_name).await
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        self.app_manager.cluster_distribution(pass_id, pass).await
    }

    pub async fn recover_js(&self, meta_pass_id_js: JsValue) {
        let meta_pass_id: MetaPasswordId = serde_wasm_bindgen::from_value(meta_pass_id_js)
            .unwrap();
        self.recover(meta_pass_id).await;
    }

    async fn recover(&self, meta_pass_id: MetaPasswordId) {
        self.app_manager.recover(meta_pass_id).await
    }
}

pub struct WasmStateUpdateManager {
    pub js_app_state: JsAppState,
}

#[async_trait(? Send)]
impl StateUpdateManager for WasmStateUpdateManager {
    async fn update_state(&self, new_state: ApplicationState) {
        let new_state_js = serde_wasm_bindgen::to_value(&new_state).unwrap();
        self.js_app_state.updateJsState(new_state_js).await;
    }
}
