use wasm_bindgen_futures::spawn_local;
use meta_secret_core::node::app::app_state_manager::{ApplicationStateManager, StateUpdateManager};
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use crate::wasm_repo::{WasmMetaLogger, WasmRepo};
use async_trait::async_trait;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use crate::{configure, JsAppState};
use meta_secret_core::models::application_state::ApplicationState;
use meta_secret_core::models::meta_password_id::MetaPasswordId;
use std::rc::Rc;
use meta_secret_core::node::logger::LoggerId;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::db::meta_db::meta_db_service::MetaDbService;
use crate::wasm_server::WasmServer;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;

#[wasm_bindgen]
pub struct WasmApplicationStateManager {
    app_manager: ApplicationStateManager<WasmRepo, WasmMetaLogger, WasmStateUpdateManager>,
    meta_db_service: Rc<MetaDbService<WasmRepo, WasmMetaLogger>>
}

#[wasm_bindgen]
impl WasmApplicationStateManager {
    pub async fn new(js_app_state: JsAppState) -> WasmApplicationStateManager {
        configure();

        let client_repo = Rc::new(WasmRepo::default());
        let device_repo = Rc::new(WasmRepo::virtual_device());

        let client_logger = Rc::new(WasmMetaLogger {
            id: LoggerId::Client
        });

        let vd_logger = Rc::new(WasmMetaLogger {
            id: LoggerId::Vd1
        });

        let data_transfer = Rc::new(MpscDataTransfer::new());

        let persistent_obj = {
            let obj = PersistentObject::new(client_repo.clone(), client_logger.clone());
            Rc::new(obj)
        };

        let meta_db_service = Rc::new(MetaDbService::new(String::from("Client"), persistent_obj.clone()));

        let wasm_server = {
            let server_repo = Rc::new(WasmRepo::server());
            let server = WasmServer::new(server_repo, data_transfer.clone(), meta_db_service.clone(), persistent_obj)
                .await;
            Rc::new(server)
        };

        let update_manager = Rc::new(WasmStateUpdateManager {
            js_app_state
        });

        let app_manager = ApplicationStateManager::new(
            client_repo, device_repo,
            client_logger, vd_logger,
            wasm_server.server.clone(),
            data_transfer,
            update_manager,
            meta_db_service.clone()
        );

        WasmApplicationStateManager { app_manager, meta_db_service }
    }


    pub async fn init(mut self) -> WasmApplicationStateManager {
        let server = self.app_manager.server.clone();
        spawn_local(async move {
            let _ = server.run().await;
        });

        let meta_db_service = self.meta_db_service.clone();
        spawn_local(async move {
            meta_db_service.run().await;
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
        let client_logger_for_client_gw = ctx.logger.clone();

        spawn_local(async move {
            ApplicationStateManager::<WasmRepo, WasmMetaLogger, WasmStateUpdateManager>::run_client_gateway(data_transfer, ctx, repo, client_logger_for_client_gw).await;
        });

        self.app_manager.on_update().await;

        self
    }

    pub async fn sign_up(&mut self, vault_name: &str, device_name: &str) {
        self.app_manager.sign_up(vault_name, device_name).await
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        self.app_manager.cluster_distribution(pass_id, pass).await
    }

    pub async fn recover_js(&mut self, meta_pass_id_js: JsValue) {
        let meta_pass_id: MetaPasswordId = serde_wasm_bindgen::from_value(meta_pass_id_js)
            .unwrap();
        self.app_manager.recover(meta_pass_id).await;
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
