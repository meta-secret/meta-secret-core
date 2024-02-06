use std::sync::Arc;
use std::thread;
use tokio::runtime::Builder;
use tracing::{info, instrument, Instrument};

use meta_secret_core::node::app::app_state_update_manager::NoOpJsAppStateManager;
use meta_secret_core::node::app::meta_app::meta_client_service::{MetaClientAccessProxy, MetaClientDataTransfer, MetaClientService};
use meta_secret_core::node::app::sync_gateway::SyncGateway;
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::meta_tracing::{client_span, server_span, vd_span};
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::server::server_app::{ServerApp, ServerDataTransfer};

pub struct NativeApplicationStateManager {
    pub state_manager: Arc<NoOpJsAppStateManager>,
    pub meta_client_proxy: Arc<MetaClientAccessProxy>,
    pub data_transfer: Arc<ServerDataTransfer>,
}

impl NativeApplicationStateManager {
    pub async fn init(
        client_repo: Arc<InMemKvLogEventRepo>,
        server_repo: Arc<InMemKvLogEventRepo>,
        vd_repo: Arc<InMemKvLogEventRepo>,
    ) -> NativeApplicationStateManager {
        let server_data_transfer = Arc::new(ServerDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        NativeApplicationStateManager::server_setup(server_repo, server_data_transfer.clone()).await;

        let device_state_manager = Arc::new(NoOpJsAppStateManager {});

        NativeApplicationStateManager::virtual_device_setup(
            vd_repo,
            server_data_transfer.clone(),
            device_state_manager,
        )
            .await;

        let client_state_manager = Arc::new(NoOpJsAppStateManager {});

        NativeApplicationStateManager::client_setup(client_repo, server_data_transfer, client_state_manager).await
    }

    #[instrument(name = "Client", skip_all)]
    pub async fn client_setup(
        client_repo: Arc<InMemKvLogEventRepo>,
        server_dt: Arc<ServerDataTransfer>,
        js_app_state: Arc<NoOpJsAppStateManager>,
    ) -> NativeApplicationStateManager {
        info!("Client setup");

        let dt_meta_client = Arc::new(MetaClientDataTransfer {
            dt: MpscDataTransfer::new(),
        });
        let meta_client_proxy = Arc::new(MetaClientAccessProxy {
            dt: dt_meta_client.clone(),
        });

        //run meta client sync gateway
        let dt_for_gateway = server_dt.clone();

        let sync_gateway = Arc::new(SyncGateway {
            id: String::from("client-gateway"),
            persistent_object: Arc::new(PersistentObject::new(client_repo.clone())),
            server_dt: dt_for_gateway
        });
        let mc_sync_gateway = sync_gateway.clone();

        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async { sync_gateway.run().instrument(client_span()).await });
        });

        //run meta client service
        let js_app_state_for_client = js_app_state.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let mcs = MetaClientService {
                    data_transfer: dt_meta_client.clone(),
                    state_manager: js_app_state_for_client,
                    sync_gateway: mc_sync_gateway,
                };

                mcs.run()
                    .instrument(client_span())
                    .await
                    .expect("Meta client service failed");
            });
        });

        Self {
            state_manager: js_app_state,
            meta_client_proxy,
            data_transfer: server_dt,
        }
    }

    #[instrument(name = "Vd", skip_all)]
    pub async fn virtual_device_setup(
        vd_repo: Arc<InMemKvLogEventRepo>,
        dt: Arc<ServerDataTransfer>,
        js_app_state: Arc<NoOpJsAppStateManager>,
    ) {
        let vd_meta_client_data_transfer = Arc::new(MetaClientDataTransfer {
            dt: MpscDataTransfer::new(),
        });
        let vd_meta_client_proxy = Arc::new(MetaClientAccessProxy {
            dt: vd_meta_client_data_transfer.clone(),
        });

        //run vd meta db service
        let persistent_object = Arc::new(PersistentObject::new(vd_repo.clone()));
        let gateway = Arc::new(SyncGateway {
            id: String::from("vd-gateway"),
            persistent_object: persistent_object.clone(),
            server_dt: dt.clone(),
        });

        //run meta client service
        let mc_gw = gateway.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let meta_client_service = {
                    Arc::new(MetaClientService {
                        data_transfer: vd_meta_client_data_transfer,
                        state_manager: js_app_state.clone(),
                        sync_gateway: mc_gw,
                    })
                };

                meta_client_service.run().instrument(vd_span()).await
            });
        });

        //run virtual device
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                VirtualDevice::init(persistent_object, vd_meta_client_proxy, dt, gateway)
                    .instrument(vd_span())
                    .await
                    .expect("Virtual device failed");
            });
        });
    }

    #[instrument(name = "MetaServer", skip_all)]
    pub async fn server_setup(server_repo: Arc<InMemKvLogEventRepo>, server_dt: Arc<ServerDataTransfer>) {
        info!("Server initialization");

        //run server
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                ServerApp::init(server_repo.clone())
                    .await
                    .unwrap()
                    .run(server_dt)
                    .instrument(server_span())
                    .await
                    .unwrap();
            });
        });
    }
}