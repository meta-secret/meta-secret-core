use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

use crate::models::ApplicationState;
use crate::node::app::client_meta_app::MetaClient;
use crate::node::app::meta_app::meta_app_service::MetaClientService;
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::app::virtual_device::VirtualDevice;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::task_runner::TaskRunner;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_service::{MetaDbService, MetaDbServiceTaskRunner};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::MetaLogger;
use crate::node::server::data_sync::{DataSync, DataSyncMessage};
use crate::node::server::server_app::ServerApp;

pub struct ApplicationStateManagerConfigurator<Repo, Logger, StateManager, Runner>
where
    Repo: KvLogEventRepo,
    Logger: MetaLogger,
    StateManager: JsAppStateManager,
    Runner: TaskRunner,
{
    pub client_repo: Arc<Repo>,
    pub server_repo: Arc<Repo>,
    pub device_repo: Arc<Repo>,

    pub client_logger: Arc<Logger>,
    pub server_logger: Arc<Logger>,
    pub device_logger: Arc<Logger>,

    pub js_app_state: Arc<StateManager>,
    pub vd_js_app_state: Arc<StateManager>,

    pub task_runner: Arc<Runner>,
}

#[async_trait(? Send)]
pub trait JsAppStateManager {
    async fn update_js_state(&self, new_state: ApplicationState);
}

pub struct ApplicationStateManager<Repo, Logger, StateManager>
where
    Repo: KvLogEventRepo,
    Logger: MetaLogger,
    StateManager: JsAppStateManager + 'static,
{
    pub state_manager: Arc<StateManager>,
    pub meta_client_service: Arc<MetaClientService<Repo, Logger, StateManager>>,
    pub client_logger: Arc<Logger>,
    pub data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
    pub meta_db_service: Arc<MetaDbService<Repo, Logger>>,
    pub sync_gateway: Arc<SyncGateway<Repo, Logger>>,
}

impl<Repo, Logger, State> ApplicationStateManager<Repo, Logger, State>
where
    Repo: KvLogEventRepo,
    Logger: MetaLogger,
    State: JsAppStateManager + 'static,
{
    pub fn new(
        meta_db_service: Arc<MetaDbService<Repo, Logger>>,
        logger: Arc<Logger>,
        data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
        state: Arc<State>,
        sync_gateway: Arc<SyncGateway<Repo, Logger>>,
        meta_client_service: Arc<MetaClientService<Repo, Logger, State>>,
    ) -> ApplicationStateManager<Repo, Logger, State> {
        logger.info("New. Application State Manager");

        ApplicationStateManager {
            meta_db_service,
            client_logger: logger,
            data_transfer,
            state_manager: state,
            sync_gateway,
            meta_client_service,
        }
    }

    pub async fn init<Runner: TaskRunner>(
        cfg: ApplicationStateManagerConfigurator<Repo, Logger, State, Runner>,
    ) -> ApplicationStateManager<Repo, Logger, State> {
        cfg.client_logger.info("Initialize application state manager");

        let data_transfer = Arc::new(MpscDataTransfer::new());

        ApplicationStateManager::<Repo, Logger, State>::server_setup(
            cfg.server_repo,
            data_transfer.clone(),
            cfg.task_runner.clone(),
            cfg.server_logger,
        )
        .await;

        ApplicationStateManager::<Repo, Logger, State>::virtual_device_setup(
            cfg.device_repo,
            data_transfer.clone(),
            cfg.task_runner.clone(),
            cfg.device_logger,
            cfg.vd_js_app_state,
        )
        .await;

        let app_manager = ApplicationStateManager::<Repo, Logger, State>::client_setup(
            cfg.client_repo,
            data_transfer.clone(),
            cfg.task_runner.clone(),
            cfg.js_app_state,
            cfg.client_logger,
        )
        .await;

        app_manager
    }

    pub async fn client_setup<Runner: TaskRunner>(
        client_repo: Arc<Repo>,
        data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
        task_runner: Arc<Runner>,
        js_app_state: Arc<State>,
        client_logger: Arc<Logger>,
    ) -> ApplicationStateManager<Repo, Logger, State> {
        let persistent_obj = {
            let obj = PersistentObject::new(client_repo.clone(), client_logger.clone());
            Arc::new(obj)
        };

        let meta_db_service = Arc::new(MetaDbService::new(String::from("Client"), persistent_obj.clone()));

        let sync_gateway = Arc::new(SyncGateway::new(
            client_repo,
            meta_db_service.clone(),
            data_transfer.clone(),
            String::from("client-gateway"),
            client_logger.clone(),
        ));

        let meta_db_service_runner = MetaDbServiceTaskRunner {
            meta_db_service: meta_db_service.clone(),
            task_runner: task_runner.clone(),
        };
        meta_db_service_runner.run_task().await;

        let meta_client_service = {
            let meta_client = Arc::new(MetaClient {
                logger: client_logger.clone(),
                persistent_obj,
                meta_db_service: meta_db_service.clone(),
            });

            Arc::new(MetaClientService {
                data_transfer: Arc::new(MpscDataTransfer::new()),
                meta_client: meta_client.clone(),
                state_manager: js_app_state.clone(),
                logger: client_logger.clone(),
            })
        };

        let app_manager = ApplicationStateManager::<Repo, Logger, State>::new(
            meta_db_service,
            client_logger,
            data_transfer,
            js_app_state,
            sync_gateway,
            meta_client_service.clone(),
        );

        let meta_client_service_runner = meta_client_service.clone();
        task_runner
            .clone()
            .spawn(async move {
                meta_client_service_runner.run().await;
            })
            .await;

        let sync_gateway_rc = app_manager.sync_gateway.clone();
        task_runner
            .clone()
            .spawn(async move {
                sync_gateway_rc.run().await;
            })
            .await;

        app_manager
    }

    pub async fn virtual_device_setup<Runner: TaskRunner>(
        device_repo: Arc<Repo>,
        data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
        task_runner: Arc<Runner>,
        device_logger: Arc<Logger>,
        js_app_state: Arc<State>,
    ) {
        device_logger.info("Device initialization");

        let persistent_object = Arc::new(PersistentObject::new(device_repo.clone(), device_logger.clone()));

        let meta_db_service = MetaDbService::new(String::from("virtual_device"), persistent_object.clone());
        let meta_db_service = Arc::new(meta_db_service);
        let vd_meta_db_service = meta_db_service.clone();

        let meta_client_service = {
            let meta_client = Arc::new(MetaClient {
                logger: device_logger.clone(),
                persistent_obj: persistent_object.clone(),
                meta_db_service: meta_db_service.clone(),
            });

            Arc::new(MetaClientService {
                data_transfer: Arc::new(MpscDataTransfer::new()),
                meta_client: meta_client.clone(),
                state_manager: js_app_state.clone(),
                logger: device_logger.clone(),
            })
        };

        task_runner
            .spawn(async move {
                vd_meta_db_service.run().await;
            })
            .await;

        let meta_client_service_runner = meta_client_service.clone();
        task_runner
            .spawn(async move {
                meta_client_service_runner.run().await;
            })
            .await;

        task_runner
            .spawn(async move {
                VirtualDevice::event_handler(
                    persistent_object,
                    meta_client_service,
                    meta_db_service,
                    data_transfer,
                    device_logger,
                )
                .await;
            })
            .await;
    }

    pub async fn server_setup<Runner: TaskRunner>(
        server_repo: Arc<Repo>,
        data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
        task_runner: Arc<Runner>,
        server_logger: Arc<Logger>,
    ) {
        server_logger.info("Server initialization");

        let server_persistent_obj = {
            let obj = PersistentObject::new(server_repo.clone(), server_logger.clone());
            Arc::new(obj)
        };

        let meta_db_service = Arc::new(MetaDbService::new(
            String::from("Server"),
            server_persistent_obj.clone(),
        ));
        let server_meta_db_service = meta_db_service.clone();

        task_runner
            .spawn(async move {
                meta_db_service.run().await;
            })
            .await;

        let data_sync = Arc::new(DataSync::new(server_persistent_obj.clone(), server_logger.clone()).await);

        let server = Arc::new(ServerApp {
            timeout: Duration::from_secs(1),
            data_sync,
            data_transfer: data_transfer.clone(),
            logger: server_logger.clone(),
            meta_db_service: server_meta_db_service,
        });

        let server_async = server.clone();
        task_runner
            .spawn(async move {
                server_async.run().await;
            })
            .await;
    }
}
