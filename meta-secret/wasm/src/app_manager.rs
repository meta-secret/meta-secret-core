use anyhow::Context;
use std::sync::Arc;
use tracing::{info, instrument, Instrument};
use wasm_bindgen_futures::spawn_local;

use meta_secret_core::node::app::app_state_update_manager::ApplicationManagerConfigurator;

use meta_secret_core::node::app::meta_app::meta_client_service::{
    MetaClientAccessProxy, MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
};
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::SyncProtocol;
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::meta_tracing::{client_span, server_span, vd_span};
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use meta_secret_core::node::server::server_app::ServerApp;

pub struct ApplicationManager<Repo: KvLogEventRepo, Sync: SyncProtocol> {
    pub meta_client_service: Arc<MetaClientService<Repo, Sync>>,
    pub server_dt: Arc<ServerDataTransfer>,
    pub sync_gateway: Arc<SyncGateway<Repo>>,
}

impl<Repo: KvLogEventRepo> ApplicationManager<Repo> {
    pub fn new(
        sync_gateway: Arc<SyncGateway<Repo>>,
        meta_client_service: Arc<MetaClientService<Repo>>,
    ) -> ApplicationManager<Repo> {
        info!("New. Application State Manager");

        ApplicationManager {
            server_dt,
            sync_gateway,
            meta_client_service,
        }
    }

    pub async fn init(
        cfg: ApplicationManagerConfigurator<Repo>,
    ) -> anyhow::Result<ApplicationManager<Repo>> {
        info!("Initialize application state manager");

        let server_dt = Arc::new(ServerDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        Self::server_setup(cfg.server_repo, server_dt.clone()).await?;

        Self::virtual_device_setup(cfg.device_repo, server_dt.clone()).await?;

        let app_manager = Self::client_setup(cfg.client_repo, server_dt.clone()).await?;

        Ok(app_manager)
    }

    #[instrument(name = "MetaClient", skip_all)]
    pub async fn client_setup(
        client_repo: Arc<Repo>,
        dt: Arc<ServerDataTransfer>,
    ) -> anyhow::Result<ApplicationManager<Repo>> {
        let persistent_obj = {
            let obj = PersistentObject::new(client_repo.clone());
            Arc::new(obj)
        };

        let sync_gateway = Arc::new(SyncGateway {
            id: String::from("client-gateway"),
            p_obj: persistent_obj.clone(),
            server_dt: dt.clone(),
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());

        let meta_client_service = {
            Arc::new(MetaClientService {
                data_transfer: Arc::new(MetaClientDataTransfer {
                    dt: MpscDataTransfer::new(),
                }),
                sync_gateway: sync_gateway.clone(),
                state_provider,
            })
        };

        let app_manager = ApplicationManager::new(dt, sync_gateway, meta_client_service.clone());

        spawn_local(async move {
            meta_client_service
                .run()
                .instrument(client_span())
                .await
                .with_context(|| "Meta client error")
                .unwrap();
        });

        let sync_gateway_rc = app_manager.sync_gateway.clone();
        spawn_local(async move { sync_gateway_rc.run().instrument(client_span()).await });

        Ok(app_manager)
    }

    #[instrument(name = "Vd", skip_all)]
    pub async fn virtual_device_setup(
        device_repo: Arc<Repo>,
        dt: Arc<ServerDataTransfer>,
    ) -> anyhow::Result<()> {
        info!("virtual device initialization");

        let persistent_object = Arc::new(PersistentObject::new(device_repo.clone()));

        let creds_repo = PersistentCredentials {
            p_obj: persistent_object.clone(),
        };

        let _user_creds = creds_repo
            .get_or_generate_user_creds(DeviceName::virtual_device(), VaultName::test())
            .await?;

        let dt_meta_client = Arc::new(MetaClientDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        let gateway = Arc::new(SyncGateway {
            id: String::from("vd-gateway"),
            p_obj: persistent_object.clone(),
            server_dt: dt.clone(),
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());
        let meta_client_service = MetaClientService {
            data_transfer: dt_meta_client.clone(),
            sync_gateway: gateway.clone(),
            state_provider,
        };

        spawn_local(async move {
            meta_client_service
                .run()
                .instrument(vd_span())
                .await
                .unwrap();
        });

        let meta_client_access_proxy = Arc::new(MetaClientAccessProxy { dt: dt_meta_client });
        let vd =
            VirtualDevice::init(persistent_object, meta_client_access_proxy, dt, gateway).await?;
        let vd = Arc::new(vd);
        spawn_local(async move { vd.run().instrument(vd_span()).await.unwrap() });

        Ok(())
    }

    #[instrument(name = "MetaServer", skip_all)]
    pub async fn server_setup(
        server_repo: Arc<Repo>,
        server_dt: Arc<ServerDataTransfer>,
    ) -> anyhow::Result<()> {
        info!("Server initialization");

        spawn_local(async move {
            ServerApp::new(server_repo.clone(), server_dt)
                .unwrap()
                .run()
                .instrument(server_span())
                .await
                .unwrap()
        });

        Ok(())
    }
}
