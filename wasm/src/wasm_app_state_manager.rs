use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use js_sys::Math::log;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::spawn_local;

use meta_secret_core::models::application_state::ApplicationState;
use meta_secret_core::models::MetaPasswordId;
use meta_secret_core::node::app::app_state_manager::{ApplicationStateManager, JsAppStateManager};
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::db::meta_db::meta_db_service::MetaDbService;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::logger::LoggerId;
use meta_secret_core::node::server::data_sync::DataSync;
use meta_secret_core::node::server::server_app::ServerApp;
use meta_secret_core::node::app::sync_gateway::SyncGateway;
use meta_secret_core::node::common::task_runner::TaskRunner;
use meta_secret_core::node::db::generic_db::KvLogEventRepo;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;

use crate::{configure, info, JsAppState};
use crate::utils::WasmTaskRunner;
use crate::wasm_repo::{WasmMetaLogger, WasmRepo};
use meta_secret_core::node::db::meta_db::meta_db_service::MetaDbServiceTaskRunner;

pub struct WasmJsAppStateManager {

}

#[async_trait(? Send)]
impl JsAppStateManager for WasmJsAppStateManager {
    async fn update_js_state(&self, _new_state: ApplicationState) {
        info("Update js state!!!");
    }
}

pub struct JsJsAppStateManager {
    js_app_state: JsAppState
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
    app_manager: GenericApplicationStateManager
}

pub enum GenericApplicationStateManager {
    Wasm {
        app_state_manager: ApplicationStateManager<WasmRepo, WasmMetaLogger, JsJsAppStateManager>,
    },
    InMem {
        app_state_manager: ApplicationStateManager<InMemKvLogEventRepo, WasmMetaLogger, WasmJsAppStateManager>,
    }
}

#[wasm_bindgen]
impl WasmApplicationStateManager {

    pub async fn init_in_mem() -> WasmApplicationStateManager {
        let client_repo = Arc::new(InMemKvLogEventRepo::default());
        let server_repo = Arc::new(InMemKvLogEventRepo::default());
        let device_repo = Arc::new(InMemKvLogEventRepo::default());

        let js_state_manager = Arc::new(WasmJsAppStateManager {});

        let app_state_manager = Self::init(client_repo, server_repo, device_repo, js_state_manager).await;
        WasmApplicationStateManager {
            app_manager: GenericApplicationStateManager::InMem { app_state_manager }
        }
    }

    pub async fn init_wasm(js_app_state: JsAppState) -> WasmApplicationStateManager {
        let client_repo = Arc::new(WasmRepo::default());
        let server_repo = Arc::new(WasmRepo::server());
        let device_repo = Arc::new(WasmRepo::virtual_device());

        let js_state_manager = Arc::new(JsJsAppStateManager {
            js_app_state
        });

        let app_state_manager = Self::init(client_repo, server_repo, device_repo, js_state_manager).await;
        WasmApplicationStateManager {
            app_manager: GenericApplicationStateManager::Wasm { app_state_manager }
        }
    }

    async fn init<Repo: KvLogEventRepo, State: JsAppStateManager>(
        client_repo: Arc<Repo>, server_repo: Arc<Repo>, device_repo: Arc<Repo>,
        js_app_state: Arc<State>) -> ApplicationStateManager<Repo, WasmMetaLogger, State>
    {
        configure();

        let data_transfer = Arc::new(MpscDataTransfer::new());
        let task_runner = Arc::new(WasmTaskRunner {});

        let app_manager = WasmApplicationStateManager::client_setup(client_repo, data_transfer.clone(), task_runner.clone(), js_app_state)
            .await;

        let sync_gateway_rc = app_manager.sync_gateway.clone();
        task_runner.spawn(async move {
            sync_gateway_rc.run().await;
        }).await;


        WasmApplicationStateManager::server_setup(server_repo, data_transfer.clone(), task_runner.clone()).await;
        WasmApplicationStateManager::virtual_device_setup(device_repo, data_transfer, task_runner);

        app_manager.setup_meta_client().await;
        app_manager.on_update().await;

        app_manager
    }

    fn virtual_device_setup<Repo: KvLogEventRepo>(device_repo: Arc<Repo>, data_transfer: Arc<MpscDataTransfer>, task_runner: Arc<WasmTaskRunner>) {
        let logger = Arc::new(WasmMetaLogger {
            id: LoggerId::Vd1
        });

        let persistent_object = Arc::new(PersistentObject::new(device_repo.clone(), logger.clone()));

        let meta_db_service = MetaDbService::new(
            String::from("virtual_device"),
            persistent_object.clone(),
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

    async fn server_setup<Repo: KvLogEventRepo>(server_repo: Arc<Repo>, data_transfer: Arc<MpscDataTransfer>, task_runner: Arc<WasmTaskRunner>) {
        let server_logger = Arc::new(WasmMetaLogger {
            id: LoggerId::Server
        });

        let server_persistent_obj = {
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

    async fn client_setup<Repo: KvLogEventRepo, State: JsAppStateManager>(
        client_repo: Arc<Repo>,
        data_transfer: Arc<MpscDataTransfer>,
        task_runner: Arc<WasmTaskRunner>,
        js_app_state: Arc<State>,
    ) -> ApplicationStateManager<Repo, WasmMetaLogger, State> {
        let client_logger = Arc::new(WasmMetaLogger {
            id: LoggerId::Client
        });

        let persistent_obj = {
            let obj = PersistentObject::new(client_repo.clone(), client_logger.clone());
            Arc::new(obj)
        };

        let meta_db_service = Arc::new(MetaDbService::new(
            String::from("Client"),
            persistent_obj.clone(),
        ));
        let client_meta_db_service = meta_db_service.clone();

        let sync_gateway = Arc::new(SyncGateway::new(
            client_repo, meta_db_service.clone(), data_transfer.clone(), String::from("client-gateway"), client_logger.clone(),
        ));

        let meta_db_service_runner = MetaDbServiceTaskRunner {
            meta_db_service: client_meta_db_service.clone(),
            task_runner: task_runner.clone()
        };
        meta_db_service_runner.run_task().await;

        ApplicationStateManager::new(
            persistent_obj,
            client_meta_db_service,
            client_logger,
            data_transfer,
            js_app_state,
            sync_gateway,
        )
    }

    pub async fn sign_up(&self, vault_name: &str, device_name: &str) {
        match &self.app_manager {
            GenericApplicationStateManager::Wasm { app_state_manager } => {
                app_state_manager.sign_up(vault_name, device_name).await
            }
            GenericApplicationStateManager::InMem { app_state_manager } => {
                app_state_manager.sign_up(vault_name, device_name).await
            }
        }
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        match &self.app_manager {
            GenericApplicationStateManager::Wasm { app_state_manager } => {
                app_state_manager.cluster_distribution(pass_id, pass).await
            }
            GenericApplicationStateManager::InMem { app_state_manager } => {
                app_state_manager.cluster_distribution(pass_id, pass).await
            }
        }
    }

    pub async fn recover_js(&self, meta_pass_id_js: JsValue) {
        let meta_pass_id: MetaPasswordId = serde_wasm_bindgen::from_value(meta_pass_id_js)
            .unwrap();

        match &self.app_manager {
            GenericApplicationStateManager::Wasm { app_state_manager } => {
                app_state_manager.recover(meta_pass_id).await;
            }
            GenericApplicationStateManager::InMem { app_state_manager } => {
                app_state_manager.recover(meta_pass_id).await;
            }
        }
    }
}

