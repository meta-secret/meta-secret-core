use std::sync::Arc;
use std::thread;
use tokio::runtime::Builder;
use tracing::{info, instrument, Instrument};

use meta_secret_core::node::app::app_state_update_manager::NoOpJsAppStateManager;
use meta_secret_core::node::app::client_meta_app::MetaClient;
use meta_secret_core::node::app::meta_app::meta_client_service::{
    MetaClientAccessProxy, MetaClientDataTransfer, MetaClientService,
};
use meta_secret_core::node::app::credentials_repo::DeviceCredentialsManager;
use meta_secret_core::node::app::sync_gateway::SyncGateway;
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::meta_tracing::{client_span, server_span, vd_span};
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::read_db::read_db_service::{ReadDbDataTransfer, ReadDbService, ReadDbServiceProxy};
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::server::data_sync::ServerDataSync;
use meta_secret_core::node::server::server_app::{ServerApp, ServerDataTransfer};

pub struct NativeApplicationStateManager {
    pub state_manager: Arc<NoOpJsAppStateManager>,
    pub meta_client_proxy: Arc<MetaClientAccessProxy>,
    pub server_data_transfer: Arc<ServerDataTransfer>,
}

impl NativeApplicationStateManager {
    pub async fn init(
        client_repo: Arc<InMemKvLogEventRepo>,
        server_repo: Arc<InMemKvLogEventRepo>,
        vd_repo: Arc<InMemKvLogEventRepo>,
    ) -> anyhow::Result<NativeApplicationStateManager> {
        let server_data_transfer = Arc::new(ServerDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        NativeApplicationStateManager::server_setup(server_repo, server_data_transfer.clone()).await?;

        let device_state_manager = Arc::new(NoOpJsAppStateManager {});

        NativeApplicationStateManager::virtual_device_setup(
            vd_repo,
            server_data_transfer.clone(),
            device_state_manager,
        )
        .await?;

        let client_state_manager = Arc::new(NoOpJsAppStateManager {});

        NativeApplicationStateManager::client_setup(client_repo, server_data_transfer, client_state_manager).await
    }

    #[instrument(name = "Client", skip_all)]
    pub async fn client_setup(
        client_repo: Arc<InMemKvLogEventRepo>,
        server_dt: Arc<ServerDataTransfer>,
        js_app_state: Arc<NoOpJsAppStateManager>,
    ) -> anyhow::Result<NativeApplicationStateManager> {
        info!("Client setup");

        let dt_meta_client = Arc::new(MetaClientDataTransfer {
            dt: MpscDataTransfer::new(),
        });
        let meta_client_proxy = Arc::new(MetaClientAccessProxy {
            dt: dt_meta_client.clone(),
        });

        let read_db_data_transfer = Arc::new(ReadDbDataTransfer {
            dt: MpscDataTransfer::new(),
        });
        let read_db_service_proxy = Arc::new(ReadDbServiceProxy {
            dt: read_db_data_transfer.clone(),
        });

        //run meta db service
        let client_repo_for_read_db = client_repo.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let persistent_obj = {
                    let obj = PersistentObject::new(client_repo_for_read_db);
                    Arc::new(obj)
                };

                let read_db_service = Arc::new(ReadDbService {
                    persistent_obj: persistent_obj.clone(),
                    repo: persistent_obj.repo.clone(),
                    read_db_id: String::from("Client"),
                    data_transfer: read_db_data_transfer,
                });
                read_db_service.run().instrument(client_span()).await
            });
        });

        //run meta client sync gateway
        let dt_for_gateway = server_dt.clone();
        let proxy_for_sync_gw = read_db_service_proxy.clone();

        let sync_gateway = Arc::new(SyncGateway {
            id: String::from("client-gateway"),
            persistent_object: Arc::new(PersistentObject::new(client_repo.clone())),
            server_dt: dt_for_gateway,
            read_db_service_proxy: proxy_for_sync_gw,
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
                let persistent_obj = {
                    let obj = PersistentObject::new(client_repo.clone());
                    Arc::new(obj)
                };

                let meta_client = Arc::new(MetaClient {
                    persistent_obj,
                    read_db_service_proxy,
                });

                let mcs = MetaClientService {
                    data_transfer: dt_meta_client.clone(),
                    meta_client: meta_client.clone(),
                    state_manager: js_app_state_for_client,
                    sync_gateway: mc_sync_gateway,
                    user_creds: creds,
                };

                mcs.run().instrument(client_span()).await;
            });
        });

        Ok(Self {
            state_manager: js_app_state,
            meta_client_proxy,
            server_data_transfer: server_dt,
        })
    }

    #[instrument(name = "Vd", skip_all)]
    pub async fn virtual_device_setup(
        vd_repo: Arc<InMemKvLogEventRepo>,
        dt: Arc<ServerDataTransfer>,
        js_app_state: Arc<NoOpJsAppStateManager>,
    ) -> anyhow::Result<()> {
        let vd_read_db_data_transfer = Arc::new(ReadDbDataTransfer {
            dt: MpscDataTransfer::new(),
        });
        let vd_read_db_service_proxy = Arc::new(ReadDbServiceProxy {
            dt: vd_read_db_data_transfer.clone(),
        });

        let vd_meta_client_data_transfer = Arc::new(MetaClientDataTransfer {
            dt: MpscDataTransfer::new(),
        });
        let vd_meta_client_proxy = Arc::new(MetaClientAccessProxy {
            dt: vd_meta_client_data_transfer.clone(),
        });

        //run vd meta db service
        let vd_repo_read_db = vd_repo.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let persistent_obj = Arc::new(PersistentObject::new(vd_repo_read_db.clone()));

                let read_db_service = ReadDbService {
                    persistent_obj: persistent_obj.clone(),
                    repo: vd_repo_read_db,
                    read_db_id: String::from("virtual_device"),
                    data_transfer: vd_read_db_data_transfer,
                };

                read_db_service.run().instrument(vd_span()).await;
            });
        });

        let persistent_object = Arc::new(PersistentObject::new(vd_repo.clone()));
        let gateway = Arc::new(SyncGateway {
            id: String::from("vd-gateway"),
            persistent_object: persistent_object.clone(),
            server_dt: dt.clone(),
            read_db_service_proxy: vd_read_db_service_proxy.clone(),
        });

        let creds = persistent_object
            .repo
            .get_or_generate_device_creds(String::from("q"), String::from("virtual-device"))
            .in_current_span()
            .await?;

        //run meta client service
        let vd_db_meta_client = vd_repo.clone();
        let service_proxy_for_client = vd_read_db_service_proxy.clone();
        let mc_gw = gateway.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let persistent_object = Arc::new(PersistentObject::new(vd_db_meta_client));

                let meta_client_service = {
                    let meta_client = Arc::new(MetaClient {
                        persistent_obj: persistent_object.clone(),
                        read_db_service_proxy: service_proxy_for_client,
                    });

                    MetaClientService {
                        data_transfer: vd_meta_client_data_transfer,
                        meta_client: meta_client.clone(),
                        state_manager: js_app_state.clone(),
                        sync_gateway: mc_gw,
                        user_creds: creds.clone(),
                    }
                };

                meta_client_service.run().instrument(vd_span()).await
            });
        });

        //run virtual device
        let vd = VirtualDevice::init(
            persistent_object, vd_meta_client_proxy, vd_read_db_service_proxy, dt, gateway, creds
        ).await?;

        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
               vd.run()
                .instrument(vd_span())
                .await
            });
        });

        Ok(())
    }

    #[instrument(name = "MetaServer", skip_all)]
    pub async fn server_setup(
        server_repo: Arc<InMemKvLogEventRepo>,
        server_dt: Arc<ServerDataTransfer>,
    ) -> anyhow::Result<()> {
        info!("Server initialization");

        let creds = {
            let persistent_obj = {
                let obj = PersistentObject::new(server_repo.clone());
                Arc::new(obj)
            };

            persistent_obj
                .repo
                .get_or_generate_device_creds(String::from("q"), String::from("server"))
                .in_current_span()
                .await?
        };

        let read_db_data_transfer = Arc::new(ReadDbDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        //run read_db service
        let dt_for_meta = read_db_data_transfer.clone();
        let server_repo_for_meta = server_repo.clone();
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let server_persistent_obj = {
                    let obj = PersistentObject::new(server_repo_for_meta.clone());
                    Arc::new(obj)
                };

                let read_db_service = ReadDbService {
                    persistent_obj: server_persistent_obj,
                    repo: server_repo_for_meta,
                    read_db_id: String::from("Server"),
                    data_transfer: dt_for_meta,
                };

                read_db_service.run().instrument(server_span()).await;
            });
        });

        //run server
        thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let server_persistent_obj = {
                    let obj = PersistentObject::new(server_repo.clone());
                    Arc::new(obj)
                };

                let read_db_service_proxy = Arc::new(ReadDbServiceProxy {
                    dt: read_db_data_transfer,
                });

                let server_data_sync = ServerDataSync::new(creds,  server_persistent_obj, read_db_service_proxy)
                    .instrument(server_span())
                    .await;
                let data_sync = server_data_sync;

                let server = Arc::new(ServerApp {
                    data_sync,
                    data_transfer: server_dt,
                });
                server.run().instrument(server_span()).await;
            });
        });

        Ok(())
    }
}
