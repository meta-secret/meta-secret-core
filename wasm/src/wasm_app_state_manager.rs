use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::spawn_local;

use meta_secret_core::models::application_state::ApplicationState;
use meta_secret_core::models::MetaPasswordId;
use meta_secret_core::node::app::app_state_manager::{ApplicationStateManager, StateUpdateManager};
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::db::meta_db::meta_db_service::MetaDbService;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::logger::LoggerId;
use meta_secret_core::node::server::data_sync::DataSync;
use meta_secret_core::node::server::server_app::ServerApp;
use meta_secret_core::node::app::sync_gateway::SyncGateway;

use crate::{configure, JsAppState};
use crate::wasm_repo::{WasmMetaLogger, WasmRepo};

#[wasm_bindgen]
pub struct WasmApplicationStateManager {
    app_manager: ApplicationStateManager<WasmRepo, WasmMetaLogger, WasmStateUpdateManager>,
}

#[wasm_bindgen]
impl WasmApplicationStateManager {
    pub async fn init(js_app_state: JsAppState) -> WasmApplicationStateManager {
        configure();

        let data_transfer = Arc::new(MpscDataTransfer::new());

        let mut app_manager = WasmApplicationStateManager::client_setup(data_transfer.clone(), js_app_state);
        WasmApplicationStateManager::server_setup(data_transfer.clone()).await;
        WasmApplicationStateManager::virtual_device_setup(data_transfer);

        app_manager.setup_meta_client().await;
        app_manager.on_update().await;

        WasmApplicationStateManager { app_manager }
    }

    fn virtual_device_setup(data_transfer: Arc<MpscDataTransfer>) {
        let logger = Arc::new(WasmMetaLogger {
            id: LoggerId::Vd1
        });

        let device_repo = Arc::new(WasmRepo::virtual_device());
        let persistent_object = Arc::new(PersistentObject::new(device_repo.clone(), logger.clone()));

        let meta_db_service = MetaDbService::new(
            String::from("virtual_device"),
            persistent_object.clone()
        );
        let meta_db_service = Arc::new(meta_db_service);
        let vd_meta_db_service = meta_db_service.clone();

        spawn_local(async move {
            vd_meta_db_service.run().await;
        });

        spawn_local(async move {
            let device_logger = Arc::new(WasmMetaLogger {
                id: LoggerId::Vd1
            });

            VirtualDevice::event_handler(
                persistent_object,
                meta_db_service,
                data_transfer,
                device_logger,
            ).await;
        });
    }

    async fn server_setup(data_transfer: Arc<MpscDataTransfer>) {
        let server_logger = Arc::new(WasmMetaLogger {
            id: LoggerId::Server
        });

        let server_persistent_obj = {
            let server_repo = Arc::new(WasmRepo::server());

            let obj = PersistentObject::new(server_repo.clone(), server_logger.clone());
            Arc::new(obj)
        };

        let meta_db_service = Arc::new(MetaDbService::new(
            String::from("Server"),
            server_persistent_obj.clone(),
        ));
        let server_meta_db_service = meta_db_service.clone();

        spawn_local(async move {
            meta_db_service.run().await;
        });

        let data_sync = Arc::new(DataSync::new(server_persistent_obj.clone(), server_logger.clone()).await);

        let server = Arc::new(ServerApp {
            timeout: Duration::from_secs(1),
            data_sync,
            data_transfer: data_transfer.mpsc_client.clone(),
            logger: server_logger.clone(),
            meta_db_service: server_meta_db_service,
        });

        let server_async = server.clone();
        spawn_local(async move {
            server_async.run().await;
        });
    }

    fn client_setup(data_transfer: Arc<MpscDataTransfer>, js_app_state: JsAppState) -> ApplicationStateManager<WasmRepo, WasmMetaLogger, WasmStateUpdateManager> {
        let client_logger = Arc::new(WasmMetaLogger {
            id: LoggerId::Client
        });

        let client_repo = Arc::new(WasmRepo::default());

        let persistent_obj = {
            let obj = PersistentObject::new(client_repo.clone(), client_logger.clone());
            Arc::new(obj)
        };

        let meta_db_service = Arc::new(MetaDbService::new(
            String::from("Client"),
            persistent_obj.clone(),
        ));
        let client_meta_db_service = meta_db_service.clone();
        let gateway_meta_db_service = meta_db_service.clone();

        spawn_local(async move {
            meta_db_service.run().await;
        });

        let gateway_client_logger = client_logger.clone();
        let gateway_data_transfer = data_transfer.clone();
        spawn_local(async move {
            let sync_gateway = SyncGateway::new(
                client_repo, gateway_data_transfer, String::from("client-gateway"), gateway_client_logger
            );
            sync_gateway.run(gateway_meta_db_service).await;
        });

        let update_manager = Arc::new(WasmStateUpdateManager {
            js_app_state
        });

        ApplicationStateManager::new(
            persistent_obj,
            client_meta_db_service,
            client_logger,
            data_transfer,
            update_manager
        )
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
