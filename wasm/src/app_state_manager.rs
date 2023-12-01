use std::sync::Arc;

use tracing::{info, instrument, Instrument};
use wasm_bindgen_futures::spawn_local;

use meta_secret_core::node::app::app_state_update_manager::{
    ApplicationStateManagerConfigurator, JsAppStateManager,
};
use meta_secret_core::node::app::client_meta_app::MetaClient;
use meta_secret_core::node::app::meta_app::meta_client_service::{
    MetaClientAccessProxy, MetaClientDataTransfer, MetaClientService,
};
use meta_secret_core::node::app::credentials_repo::CredentialsRepo;
use meta_secret_core::node::app::sync_gateway::SyncGateway;
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::meta_tracing::{client_span, server_span, vd_span};
use meta_secret_core::node::common::model::device::DeviceName;
use meta_secret_core::node::common::model::vault::VaultName;
use meta_secret_core::node::db::events::generic_log_event::UnitEvent;
use meta_secret_core::node::db::events::local::CredentialsObject;
use meta_secret_core::node::db::generic_db::KvLogEventRepo;
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
    pub sync_gateway: Arc<SyncGateway<Repo>>,
}

impl<Repo, State> ApplicationStateManager<Repo, State>
    where
        Repo: KvLogEventRepo,
        State: JsAppStateManager + 'static,
{
    pub fn new(
        server_dt: Arc<ServerDataTransfer>,
        state: Arc<State>,
        sync_gateway: Arc<SyncGateway<Repo>>,
        meta_client_service: Arc<MetaClientService<Repo, State>>,
    ) -> ApplicationStateManager <Repo, State> {
        info!("New. Application State Manager");

        ApplicationStateManager {
            server_dt,
            state_manager: state,
            sync_gateway,
            meta_client_service,
        }
    }

    pub async fn init(cfg: ApplicationStateManagerConfigurator<Repo, State>) -> anyhow::Result<ApplicationStateManager<Repo, State>> {
        info!("Initialize application state manager");

        let server_dt = Arc::new(ServerDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        Self::server_setup(cfg.server_repo, server_dt.clone()).await?;

        ApplicationStateManager::virtual_device_setup(
            cfg.device_repo,
            server_dt.clone(),
            cfg.vd_js_app_state,
        )
            .await?;

        let app_manager = ApplicationStateManager::client_setup(cfg.client_repo, server_dt.clone(), cfg.js_app_state)
            .await?;
        Ok(app_manager)
    }

    #[instrument(name = "MetaClient", skip(client_repo, dt, js_app_state))]
    pub async fn client_setup(
        client_repo: Arc<Repo>, dt: Arc<ServerDataTransfer>, js_app_state: Arc<State>,
    ) -> anyhow::Result<ApplicationStateManager <Repo, State>> {
        let persistent_obj = {
            let obj = PersistentObject::new(client_repo.clone());
            Arc::new(obj)
        };
        
        let sync_gateway = Arc::new(SyncGateway {
            id: String::from("client-gateway"),
            persistent_object: persistent_obj.clone(),
            server_dt: dt.clone(),
            creds: ,
        });

        let meta_client_service = {
            let meta_client = Arc::new(MetaClient {
                persistent_obj: persistent_obj.clone() 
            });

            let creds = persistent_obj
                .repo
                .get_or_generate_device_creds(String::from("client"))
                .in_current_span()
                .await?;

            Arc::new(MetaClientService {
                data_transfer: Arc::new(MetaClientDataTransfer {
                    dt: MpscDataTransfer::new(),
                }),
                meta_client: meta_client.clone(),
                state_manager: js_app_state.clone(),
                sync_gateway: sync_gateway.clone(),
                user_creds: creds,
            })
        };

        let app_manager = ApplicationStateManager::new(
            read_db_service,
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

        Ok(app_manager)
    }

    #[instrument(name = "Vd", skip(device_repo, dt, js_app_state))]
    pub async fn virtual_device_setup(
        device_repo: Arc<Repo>,
        dt: Arc<ServerDataTransfer>,
        js_app_state: Arc<State>,
    ) -> anyhow::Result<()> {
        info!("Device initialization");

        let persistent_object = Arc::new(PersistentObject::new(device_repo.clone()));

        let creds_repo = CredentialsRepo {
            repo: persistent_object.repo.clone(),
        };

        let creds = creds_repo
            .get_or_generate_user_creds(DeviceName::from("virtual-device"), VaultName::from("q"))
            .await?;

        let vd_read_db_data_transfer = Arc::new(ReadDbDataTransfer {
            dt: MpscDataTransfer::new(),
        });
        let vd_read_db_service_proxy = Arc::new(ReadDbServiceProxy {
            dt: vd_read_db_data_transfer.clone(),
        });

        let read_db_service = ReadDbService {
            persistent_obj: persistent_object.clone(),
            repo: device_repo,
            read_db_id: String::from("virtual_device"),
            data_transfer: vd_read_db_data_transfer.clone(),
        };

        let read_db_service_proxy = Arc::new(ReadDbServiceProxy {
            dt: vd_read_db_data_transfer,
        });

        let read_db_service = Arc::new(read_db_service);

        let vd_read_db_service = read_db_service.clone();

        let dt_meta_client = Arc::new(MetaClientDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        let creds_obj = CredentialsObject::unit(creds);
        let gateway = Arc::new(SyncGateway {
            id: String::from("vd-gateway"),
            persistent_object: persistent_object.clone(),
            server_dt: dt.clone(),
            read_db_service_proxy: vd_read_db_service_proxy.clone(),
            creds: creds_obj.clone(),
        });

        let meta_client_service = {
            let meta_client = Arc::new(MetaClient {
                persistent_obj: persistent_object.clone(),
                read_db_service_proxy: read_db_service_proxy.clone(),
            });

            MetaClientService {
                data_transfer: dt_meta_client.clone(),
                meta_client: meta_client.clone(),
                state_manager: js_app_state.clone(),
                sync_gateway: gateway.clone(),
                device_creds: creds_obj.clone()
            }
        };

        let meta_client_access_proxy = Arc::new(MetaClientAccessProxy { dt: dt_meta_client });
        let vd = VirtualDevice::init(
            persistent_object,
            meta_client_access_proxy,
            read_db_service_proxy,
            dt,
            gateway,
            creds
        )
            .await?;
        let vd = Arc::new(vd);

        spawn_local(async move {
            vd_read_db_service.run().instrument(vd_span()).await;
        });

        spawn_local(async move { meta_client_service.run().instrument(vd_span()).await });

        spawn_local(async move { vd.run().instrument(vd_span()).await });

        Ok(())
    }

    #[instrument(name = "MetaServer", skip_all)]
    pub async fn server_setup(server_repo: Arc<Repo>, server_dt: Arc<ServerDataTransfer>) -> anyhow::Result<()> {
        info!("Server initialization");

        let server_persistent_obj = {
            let obj = PersistentObject::new(server_repo.clone());
            Arc::new(obj)
        };

        let device_creds = server_persistent_obj
            .repo
            .get_or_generate_device_creds(String::from("server"))
            .in_current_span()
            .await?;
        
        let data_sync = ServerDataSync {
            persistent_obj: server_persistent_obj.clone(),
            device_creds,
        };

        let server = ServerApp {
            data_sync,
            data_transfer: server_dt.clone(),
            device_creds: DeviceCredentials {},
        };

        spawn_local(async move { server.run().instrument(server_span()).await });

        Ok(())
    }
}
