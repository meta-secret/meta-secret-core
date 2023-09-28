use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use async_mutex::Mutex;
use tokio::runtime::Builder;
use tracing::info;

use meta_secret_core::node::app::app_state_update_manager::NoOpJsAppStateManager;
use meta_secret_core::node::app::client_meta_app::MetaClient;
use meta_secret_core::node::app::meta_app::meta_app_service::{MetaClientAccessProxy, MetaClientService};
use meta_secret_core::node::app::sync_gateway::SyncGateway;
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::meta_db::meta_db_service::{MetaDbService, MetaDbServiceProxy};
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::server::data_sync::{DataSyncMessage, ServerDataSync};
use meta_secret_core::node::server::server_app::ServerApp;

pub struct NativeApplicationStateManager {
    pub state_manager: Arc<NoOpJsAppStateManager>,
    pub meta_client_proxy: Arc<MetaClientAccessProxy>,
    pub data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
}

impl NativeApplicationStateManager {
    pub async fn init(server_db: Arc<Mutex<HashMap<ObjectId, GenericKvLogEvent>>>) -> NativeApplicationStateManager {
        let server_data_transfer = Arc::new(MpscDataTransfer::new());

        NativeApplicationStateManager::server_setup(server_db.clone(), server_data_transfer.clone()).await;

        let device_state_manager = Arc::new(NoOpJsAppStateManager {});

        let vd_db = Arc::new(Mutex::new(HashMap::default()));
        NativeApplicationStateManager::virtual_device_setup(server_data_transfer.clone(), device_state_manager, vd_db)
            .await;

        let client_state_manager = Arc::new(NoOpJsAppStateManager {});

        NativeApplicationStateManager::client_setup(server_data_transfer.clone(), client_state_manager).await
    }

    pub async fn client_setup(
        server_data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
        js_app_state: Arc<NoOpJsAppStateManager>,
    ) -> NativeApplicationStateManager {
        let db = Arc::new(Mutex::new(HashMap::default()));

        let dt_meta_client = Arc::new(MpscDataTransfer::new());
        let meta_client_proxy = Arc::new(MetaClientAccessProxy {
            data_transfer: dt_meta_client.clone(),
        });

        let meta_db_data_transfer = Arc::new(MpscDataTransfer::new());
        let meta_db_service_proxy = Arc::new(MetaDbServiceProxy {
            data_transfer: meta_db_data_transfer.clone(),
        });

        //run meta client service
        let db_for_meta_client = db.clone();
        let proxy_for_meta_client = meta_db_service_proxy.clone();
        let js_app_state_for_client = js_app_state.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let client_repo = Arc::new(InMemKvLogEventRepo { db: db_for_meta_client });

                let persistent_obj = {
                    let obj = PersistentObject::new(client_repo.clone());
                    Arc::new(obj)
                };

                let meta_client = Arc::new(MetaClient {
                    persistent_obj,
                    meta_db_service_proxy: proxy_for_meta_client,
                });

                let mcs = MetaClientService {
                    data_transfer: dt_meta_client.clone(),
                    meta_client: meta_client.clone(),
                    state_manager: js_app_state_for_client,
                };

                mcs.run().await;
            });
        });

        //run meta db service
        let db_for_meta_db = db.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let client_repo = Arc::new(InMemKvLogEventRepo { db: db_for_meta_db });

                let persistent_obj = {
                    let obj = PersistentObject::new(client_repo.clone());
                    Arc::new(obj)
                };

                let meta_db_service = Arc::new(MetaDbService {
                    persistent_obj: persistent_obj.clone(),
                    repo: persistent_obj.repo.clone(),
                    meta_db_id: String::from("Client"),
                    data_transfer: meta_db_data_transfer,
                });
                meta_db_service.run().await
            });
        });

        //run meta client sync gateway
        let dt_for_gateway = server_data_transfer.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let client_repo = Arc::new(InMemKvLogEventRepo { db: db.clone() });
                let sync_gateway = Arc::new(SyncGateway {
                    id: String::from("client-gateway"),
                    repo: client_repo.clone(),
                    persistent_object: Arc::new(PersistentObject::new(client_repo.clone())),
                    server_data_transfer: dt_for_gateway,
                    meta_db_service_proxy,
                });

                sync_gateway.run().await
            });
        });

        Self {
            state_manager: js_app_state,
            meta_client_proxy,
            data_transfer: server_data_transfer,
        }
    }

    pub async fn virtual_device_setup(
        server_data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
        js_app_state: Arc<NoOpJsAppStateManager>,
        db: Arc<Mutex<HashMap<ObjectId, GenericKvLogEvent>>>,
    ) {
        let vd_meta_db_data_transfer = Arc::new(MpscDataTransfer::new());
        let vd_meta_db_service_proxy = Arc::new(MetaDbServiceProxy {
            data_transfer: vd_meta_db_data_transfer.clone(),
        });

        let vd_meta_client_data_transfer = Arc::new(MpscDataTransfer::new());
        let vd_meta_client_proxy = Arc::new(MetaClientAccessProxy {
            data_transfer: vd_meta_client_data_transfer.clone(),
        });

        //run vd meta db service
        let vd_db_meta_db = db.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let device_repo = Arc::new(InMemKvLogEventRepo { db: vd_db_meta_db });

                let persistent_obj = Arc::new(PersistentObject::new(device_repo.clone()));

                let meta_db_service = MetaDbService {
                    persistent_obj: persistent_obj.clone(),
                    repo: device_repo.clone(),
                    meta_db_id: String::from("virtual_device"),
                    data_transfer: vd_meta_db_data_transfer,
                };

                meta_db_service.run().await;
            });
        });

        //run meta client service
        let vd_db_meta_client = db.clone();
        let service_proxy_for_client = vd_meta_db_service_proxy.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let device_repo = Arc::new(InMemKvLogEventRepo { db: vd_db_meta_client });

                let persistent_object = Arc::new(PersistentObject::new(device_repo.clone()));

                let meta_client_service = {
                    let meta_client = Arc::new(MetaClient {
                        persistent_obj: persistent_object.clone(),
                        meta_db_service_proxy: service_proxy_for_client,
                    });

                    Arc::new(MetaClientService {
                        data_transfer: vd_meta_client_data_transfer,
                        meta_client: meta_client.clone(),
                        state_manager: js_app_state.clone(),
                    })
                };

                meta_client_service.run().await
            });
        });

        //run virtual device
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let device_repo = Arc::new(InMemKvLogEventRepo { db: db.clone() });

                let persistent_object = Arc::new(PersistentObject::new(device_repo.clone()));

                VirtualDevice::event_handler(
                    persistent_object,
                    vd_meta_client_proxy,
                    vd_meta_db_service_proxy,
                    server_data_transfer,
                )
                .await
            });
        });
    }

    pub async fn server_setup(
        db: Arc<Mutex<HashMap<ObjectId, GenericKvLogEvent>>>,
        server_data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
    ) {
        info!("Server initialization");

        let meta_db_data_transfer = Arc::new(MpscDataTransfer::new());

        //run meta_db service
        let db_meta = db.clone();
        let dt_for_meta = meta_db_data_transfer.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let server_repo = Arc::new(InMemKvLogEventRepo { db: db_meta });

                let server_persistent_obj = {
                    let obj = PersistentObject::new(server_repo.clone());
                    Arc::new(obj)
                };

                let meta_db_service = MetaDbService {
                    persistent_obj: server_persistent_obj,
                    repo: server_repo,
                    meta_db_id: String::from("Server"),
                    data_transfer: dt_for_meta,
                };

                meta_db_service.run().await;
            });
        });

        //run server
        let db_server = db.clone();
        let dt_for_server = meta_db_data_transfer.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let server_repo = Arc::new(InMemKvLogEventRepo { db: db_server });

                let server_persistent_obj = {
                    let obj = PersistentObject::new(server_repo.clone());
                    Arc::new(obj)
                };

                let server_data_sync = ServerDataSync::new(server_persistent_obj).await;
                let data_sync = Arc::new(server_data_sync);

                let server = Arc::new(ServerApp {
                    data_sync,
                    data_transfer: server_data_transfer,
                    meta_db_service_proxy: Arc::new(MetaDbServiceProxy {
                        data_transfer: dt_for_server,
                    }),
                });
                server.run().await;
            });
        });
    }
}
