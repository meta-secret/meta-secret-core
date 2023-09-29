use std::sync::Arc;
use tracing::{info, instrument, Instrument};

use wasm_bindgen_futures::spawn_local;

use meta_secret_core::node::app::app_state_update_manager::{
    ApplicationStateManagerConfigurator, JsAppStateManager,
};
use meta_secret_core::node::app::client_meta_app::MetaClient;
use meta_secret_core::node::app::meta_app::meta_app_service::{
    MetaClientAccessProxy, MetaClientDataTransfer, MetaClientService,
};
use meta_secret_core::node::app::sync_gateway::SyncGateway;
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::meta_tracing::{client_span, server_span, vd_span};
use meta_secret_core::node::db::generic_db::KvLogEventRepo;
use meta_secret_core::node::db::meta_db::meta_db_service::{
    MetaDbDataTransfer, MetaDbService, MetaDbServiceProxy,
};
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::server::data_sync::ServerDataSync;
use meta_secret_core::node::server::server_app::{ServerApp, ServerDataTransfer};

pub struct ApplicationStateManager<Repo, StateManager>
where
    Repo: KvLogEventRepo,

    StateManager: JsAppStateManager,
{
    pub state_manager: Arc<StateManager>,
    pub meta_client_service: Arc<MetaClientService<Repo, StateManager>>,
    pub server_dt: Arc<ServerDataTransfer>,
    pub meta_db_service: Arc<MetaDbService<Repo>>,
    pub sync_gateway: Arc<SyncGateway<Repo>>,
}

impl<Repo, State> ApplicationStateManager<Repo, State>
where
    Repo: KvLogEventRepo,

    State: JsAppStateManager + 'static,
{
    pub fn new(
        meta_db_service: Arc<MetaDbService<Repo>>,
        server_dt: Arc<ServerDataTransfer>,
        state: Arc<State>,
        sync_gateway: Arc<SyncGateway<Repo>>,
        meta_client_service: Arc<MetaClientService<Repo, State>>,
    ) -> ApplicationStateManager<Repo, State> {
        info!("New. Application State Manager");

        ApplicationStateManager {
            meta_db_service,
            server_dt,
            state_manager: state,
            sync_gateway,
            meta_client_service,
        }
    }

    pub async fn init(
        cfg: ApplicationStateManagerConfigurator<Repo, State>,
    ) -> ApplicationStateManager<Repo, State> {
        info!("Initialize application state manager");

        let server_dt = Arc::new(ServerDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        ApplicationStateManager::<Repo, State>::server_setup(cfg.server_repo, server_dt.clone())
            .await;

        ApplicationStateManager::<Repo, State>::virtual_device_setup(
            cfg.device_repo,
            server_dt.clone(),
            cfg.vd_js_app_state,
        )
        .await;

        ApplicationStateManager::<Repo, State>::client_setup(
            cfg.client_repo,
            server_dt.clone(),
            cfg.js_app_state,
        )
        .await
    }

    #[instrument(name = "MetaClient", skip(client_repo, dt, js_app_state))]
    pub async fn client_setup(
        client_repo: Arc<Repo>,
        dt: Arc<ServerDataTransfer>,
        js_app_state: Arc<State>,
    ) -> ApplicationStateManager<Repo, State> {
        let persistent_obj = {
            let obj = PersistentObject::new(client_repo.clone());
            Arc::new(obj)
        };

        let dt_meta_db = Arc::new(MetaDbDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        let meta_db_service = Arc::new(MetaDbService {
            persistent_obj: persistent_obj.clone(),
            repo: client_repo.clone(),
            meta_db_id: String::from("Client"),
            data_transfer: dt_meta_db.clone(),
        });

        let meta_db_service_proxy = Arc::new(MetaDbServiceProxy { dt: dt_meta_db });

        let sync_gateway = Arc::new(SyncGateway {
            id: String::from("client-gateway"),
            repo: client_repo,
            persistent_object: persistent_obj.clone(),
            server_dt: dt.clone(),
            meta_db_service_proxy: meta_db_service_proxy.clone(),
        });

        let meta_client_service: Arc<MetaClientService<Repo, State>> = {
            let meta_client = Arc::new(MetaClient {
                persistent_obj,
                meta_db_service_proxy: meta_db_service_proxy.clone(),
            });

            Arc::new(MetaClientService {
                data_transfer: Arc::new(MetaClientDataTransfer {
                    dt: MpscDataTransfer::new(),
                }),
                meta_client: meta_client.clone(),
                state_manager: js_app_state.clone(),
                sync_gateway: sync_gateway.clone(),
            })
        };

        let app_manager = ApplicationStateManager::new(
            meta_db_service,
            dt,
            js_app_state.clone(),
            sync_gateway,
            meta_client_service.clone(),
        );

        let meta_client_service_runner = meta_client_service.clone();
        spawn_local(async move {
            meta_client_service_runner
                .run()
                .instrument(client_span())
                .await
        });

        let sync_gateway_rc = app_manager.sync_gateway.clone();
        spawn_local(async move { sync_gateway_rc.run().instrument(client_span()).await });

        app_manager
    }

    #[instrument(name = "Vd", skip(device_repo, dt, js_app_state))]
    pub async fn virtual_device_setup(
        device_repo: Arc<Repo>,
        dt: Arc<ServerDataTransfer>,
        js_app_state: Arc<State>,
    ) {
        info!("Device initialization");

        let persistent_object = Arc::new(PersistentObject::new(device_repo.clone()));

        let vd_meta_db_data_transfer = Arc::new(MetaDbDataTransfer {
            dt: MpscDataTransfer::new(),
        });
        let vd_meta_db_service_proxy = Arc::new(MetaDbServiceProxy {
            dt: vd_meta_db_data_transfer.clone(),
        });

        let meta_db_service = MetaDbService {
            persistent_obj: persistent_object.clone(),
            repo: device_repo,
            meta_db_id: String::from("virtual_device"),
            data_transfer: vd_meta_db_data_transfer.clone(),
        };

        let meta_db_service_proxy = Arc::new(MetaDbServiceProxy {
            dt: vd_meta_db_data_transfer,
        });

        let meta_db_service = Arc::new(meta_db_service);

        let vd_meta_db_service = meta_db_service.clone();

        let dt_meta_client = Arc::new(MetaClientDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        let gateway = Arc::new(SyncGateway {
            id: String::from("vd-gateway"),
            repo: persistent_object.repo.clone(),
            persistent_object: persistent_object.clone(),
            server_dt: dt.clone(),
            meta_db_service_proxy: vd_meta_db_service_proxy.clone(),
        });

        let meta_client_service = {
            let meta_client = Arc::new(MetaClient {
                persistent_obj: persistent_object.clone(),
                meta_db_service_proxy: meta_db_service_proxy.clone(),
            });

            Arc::new(MetaClientService {
                data_transfer: dt_meta_client.clone(),
                meta_client: meta_client.clone(),
                state_manager: js_app_state.clone(),
                sync_gateway: gateway.clone(),
            })
        };

        spawn_local(async move {
            vd_meta_db_service.run().instrument(vd_span()).await;
        });

        let meta_client_service_runner = meta_client_service.clone();
        spawn_local(async move { meta_client_service_runner.run().instrument(vd_span()).await });

        spawn_local(async move {
            let meta_client_access_proxy = Arc::new(MetaClientAccessProxy { dt: dt_meta_client });

            let vd_async = VirtualDevice::event_handler(
                persistent_object,
                meta_client_access_proxy,
                meta_db_service_proxy,
                dt,
                gateway,
            );

            vd_async.instrument(vd_span()).await
        })
    }

    #[instrument(name = "MetaServer", skip_all)]
    pub async fn server_setup(server_repo: Arc<Repo>, server_dt: Arc<ServerDataTransfer>) {
        info!("Server initialization");

        let server_persistent_obj = {
            let obj = PersistentObject::new(server_repo.clone());
            Arc::new(obj)
        };

        let meta_db_dt = Arc::new(MetaDbDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        let meta_db_service_proxy = Arc::new(MetaDbServiceProxy {
            dt: meta_db_dt.clone(),
        });

        let meta_db_service = Arc::new(MetaDbService {
            persistent_obj: server_persistent_obj.clone(),
            repo: server_repo.clone(),
            meta_db_id: String::from("Server"),
            data_transfer: meta_db_dt.clone(),
        });

        spawn_local(async move {
            meta_db_service.run().instrument(server_span()).await;
        });

        let data_sync = Arc::new(
            ServerDataSync::new(server_persistent_obj.clone())
                .in_current_span()
                .await,
        );

        let server = Arc::new(ServerApp {
            data_sync,
            data_transfer: server_dt.clone(),
            meta_db_service_proxy,
        });

        let server_async = server.clone();
        spawn_local(async move { server_async.run().instrument(server_span()).await });
    }
}
