use async_trait::async_trait;
use std::cell::RefCell;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use crate::models::ApplicationState;
use crate::models::MetaPasswordId;
use crate::node::app::meta_app::{EmptyMetaClient, MetaClient, MetaClientContext, RegisteredMetaClient};
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::app::virtual_device::VirtualDevice;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::task_runner::TaskRunner;
use crate::node::db::events::common::VaultInfo;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_service::{MetaDbService, MetaDbServiceTaskRunner};
use crate::node::db::meta_db::store::meta_pass_store::MetaPassStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::MetaLogger;
use crate::node::server::data_sync::DataSync;
use crate::node::server::server_app::ServerApp;
use async_mutex::Mutex as AsyncMutex;

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
    StateManager: JsAppStateManager,
{
    pub state_manager: Arc<StateManager>,
    pub meta_client: Arc<AsyncMutex<MetaClient<Repo, Logger>>>,
    pub client_logger: Arc<Logger>,
    pub data_transfer: Arc<MpscDataTransfer>,
    pub meta_db_service: Arc<MetaDbService<Repo, Logger>>,
    pub sync_gateway: Arc<SyncGateway<Repo, Logger>>,
}

impl<Repo, Logger, State> ApplicationStateManager<Repo, Logger, State>
where
    Repo: KvLogEventRepo,
    Logger: MetaLogger,
    State: JsAppStateManager,
{
    pub async fn init<Runner: TaskRunner>(
        cfg: ApplicationStateManagerConfigurator<Repo, Logger, State, Runner>,
    ) -> ApplicationStateManager<Repo, Logger, State> {
        let data_transfer = Arc::new(MpscDataTransfer::new());

        let app_manager = {
            let app_manager = ApplicationStateManager::client_setup(
                cfg.client_repo,
                data_transfer.clone(),
                cfg.task_runner.clone(),
                cfg.js_app_state,
                cfg.client_logger,
            )
            .await;

            let sync_gateway_rc = app_manager.sync_gateway.clone();
            cfg.task_runner
                .clone()
                .spawn(async move {
                    sync_gateway_rc.run().await;
                })
                .await;

            app_manager.setup_meta_client().await;
            app_manager.on_update().await;
            app_manager
        };

        {
            ApplicationStateManager::<Repo, Logger, State>::server_setup(
                cfg.server_repo,
                data_transfer.clone(),
                cfg.task_runner.clone(),
                cfg.server_logger,
            )
            .await;
        }

        {
            ApplicationStateManager::<Repo, Logger, State>::virtual_device_setup(
                cfg.device_repo,
                data_transfer,
                cfg.task_runner,
                cfg.device_logger,
            )
            .await;
        }

        app_manager
    }

    pub async fn client_setup<Runner: TaskRunner>(
        client_repo: Arc<Repo>,
        data_transfer: Arc<MpscDataTransfer>,
        task_runner: Arc<Runner>,
        js_app_state: Arc<State>,
        client_logger: Arc<Logger>,
    ) -> ApplicationStateManager<Repo, Logger, State> {
        let persistent_obj = {
            let obj = PersistentObject::new(client_repo.clone(), client_logger.clone());
            Arc::new(obj)
        };

        let meta_db_service = Arc::new(MetaDbService::new(String::from("Client"), persistent_obj.clone()));
        let client_meta_db_service = meta_db_service.clone();

        let sync_gateway = Arc::new(SyncGateway::new(
            client_repo,
            meta_db_service.clone(),
            data_transfer.clone(),
            String::from("client-gateway"),
            client_logger.clone(),
        ));

        let meta_db_service_runner = MetaDbServiceTaskRunner {
            meta_db_service: client_meta_db_service.clone(),
            task_runner: task_runner.clone(),
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

    pub async fn virtual_device_setup<Runner: TaskRunner>(
        device_repo: Arc<Repo>,
        data_transfer: Arc<MpscDataTransfer>,
        task_runner: Arc<Runner>,
        device_logger: Arc<Logger>,
    ) {
        let persistent_object = Arc::new(PersistentObject::new(device_repo.clone(), device_logger.clone()));

        let meta_db_service = MetaDbService::new(String::from("virtual_device"), persistent_object.clone());
        let meta_db_service = Arc::new(meta_db_service);
        let vd_meta_db_service = meta_db_service.clone();

        task_runner
            .spawn(async move {
                vd_meta_db_service.run().await;
            })
            .await;

        task_runner
            .spawn(async move {
                VirtualDevice::event_handler(persistent_object, meta_db_service, data_transfer, device_logger).await;
            })
            .await;
    }

    pub async fn server_setup<Runner: TaskRunner>(
        server_repo: Arc<Repo>,
        data_transfer: Arc<MpscDataTransfer>,
        task_runner: Arc<Runner>,
        server_logger: Arc<Logger>,
    ) {
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
            data_transfer: data_transfer.mpsc_client.clone(),
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

impl<Repo, Logger, State> ApplicationStateManager<Repo, Logger, State>
where
    Repo: KvLogEventRepo,
    Logger: MetaLogger,
    State: JsAppStateManager,
{
    pub async fn sign_up(&self, vault_name: &str, device_name: &str) {
        let mut curr_meta_client = self.meta_client.lock().await;

        match curr_meta_client.deref() {
            MetaClient::Empty(client) => {
                let new_client_result = client.get_or_create_local_vault(vault_name, device_name).await;

                if let Ok(new_client) = new_client_result {
                    new_client.ctx.enable_join().await;
                    *curr_meta_client = MetaClient::Init(new_client);
                }
            }
            MetaClient::Init(client) => {
                //ignore
                *curr_meta_client = MetaClient::Registered(client.sign_up().await);
            }
            MetaClient::Registered(_) => {
                //ignore
            }
        };

        self.on_update().await;
    }

    pub async fn recover(&self, meta_pass_id: MetaPasswordId) {
        let curr_meta_client = self.meta_client.lock().await;

        match curr_meta_client.deref() {
            MetaClient::Empty(_) => {}
            MetaClient::Init(_) => {}
            MetaClient::Registered(client) => {
                client.recovery_request(meta_pass_id).await;
            }
        }
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        let curr_meta_client = self.meta_client.lock().await;

        match curr_meta_client.deref() {
            MetaClient::Empty(_) => {}
            MetaClient::Init(_) => {}
            MetaClient::Registered(client) => {
                self.client_logger.info("Password, cluster distribution");

                client.cluster_distribution(pass_id, pass).await;
                let passwords = {
                    let pass_store = self.meta_db_service.get_meta_pass_store().await.unwrap();
                    match pass_store {
                        MetaPassStore::Store { passwords, .. } => passwords.clone(),
                        _ => {
                            vec![]
                        }
                    }
                };

                {
                    let mut app_state = client.ctx.app_state.borrow_mut();
                    app_state.meta_passwords.clear();
                    app_state.meta_passwords = passwords;
                    self.on_update().await;
                }
            }
        }
    }
}

impl<Repo, Logger, State> ApplicationStateManager<Repo, Logger, State>
where
    Repo: KvLogEventRepo,
    Logger: MetaLogger,
    State: JsAppStateManager,
{
    pub fn new(
        persistent_obj: Arc<PersistentObject<Repo, Logger>>,
        meta_db_service: Arc<MetaDbService<Repo, Logger>>,
        logger: Arc<Logger>,
        data_transfer: Arc<MpscDataTransfer>,
        state: Arc<State>,
        sync_gateway: Arc<SyncGateway<Repo, Logger>>,
    ) -> ApplicationStateManager<Repo, Logger, State> {
        logger.info("New. Application State Manager");

        let ctx = {
            let app_state = {
                let state = ApplicationState {
                    meta_vault: None,
                    vault: None,
                    meta_passwords: vec![],
                    join_component: false,
                };
                RefCell::new(state)
            };

            MetaClientContext::new(app_state)
        };

        let meta_client = MetaClient::Empty(EmptyMetaClient {
            ctx: Arc::new(ctx),
            logger: logger.clone(),
            persistent_obj,
            meta_db_service: meta_db_service.clone(),
        });

        ApplicationStateManager {
            meta_db_service,
            meta_client: Arc::new(AsyncMutex::new(meta_client)),
            client_logger: logger,
            data_transfer,
            state_manager: state,
            sync_gateway,
        }
    }

    pub async fn setup_meta_client(&self) {
        self.client_logger.info("Setup meta client");

        let mut curr_meta_client = self.meta_client.lock().await;

        match curr_meta_client.deref() {
            MetaClient::Empty(client) => {
                let init_client_result = client.find_user_creds().await;
                if let Ok(Some(init_client)) = init_client_result {
                    init_client
                        .ctx
                        .update_meta_vault(init_client.creds.user_sig.vault.clone())
                        .await;

                    let vault_info = init_client.get_vault().await;
                    if let VaultInfo::Member { vault } = &vault_info {
                        let new_meta_client = init_client.sign_up().await;

                        {
                            new_meta_client.ctx.update_vault(vault.clone()).await;
                            let meta_pass_store = self.meta_db_service.get_meta_pass_store().await.unwrap();

                            new_meta_client
                                .ctx
                                .update_meta_passwords(meta_pass_store.passwords())
                                .await;
                        }
                        *curr_meta_client = MetaClient::Registered(new_meta_client);
                    } else {
                        *curr_meta_client = MetaClient::Init(init_client);
                    }
                }
            }
            MetaClient::Init(client) => {
                let new_client = client.sign_up().await;
                if let VaultInfo::Member { vault } = &new_client.vault_info {
                    MetaClientContext::update_vault(&new_client.ctx, vault.clone()).await;
                }
                *curr_meta_client = MetaClient::Registered(new_client);
            }
            MetaClient::Registered(client) => {
                self.registered_client(client).await;
            }
        }
    }

    async fn registered_client(&self, client: &RegisteredMetaClient<Repo, Logger>) {
        match &client.vault_info {
            VaultInfo::Member { vault } => {
                client.ctx.update_vault(vault.clone()).await;
            }
            VaultInfo::Pending => {
                //ignore
            }
            VaultInfo::Declined => {
                //ignore
            }
            VaultInfo::NotFound => {
                //ignore
            }
            VaultInfo::NotMember => {
                //ignore
            }
        }
    }

    pub async fn on_update(&self) {
        // update app state in vue
        self.state_manager.update_js_state(self.get_state().await).await
    }

    pub async fn get_state(&self) -> ApplicationState {
        let curr_meta_client = self.meta_client.lock().await;

        let app_state = match curr_meta_client.deref() {
            MetaClient::Empty(client) => client.ctx.app_state.borrow(),
            MetaClient::Init(client) => client.ctx.app_state.borrow(),
            MetaClient::Registered(client) => client.ctx.app_state.borrow(),
        };

        self.client_logger
            .debug(format!("Current app state for js: {:?}", app_state).as_str());

        //let new_state_js = serde_wasm_bindgen::to_value(&new_state).unwrap();
        app_state.clone()
    }
}
