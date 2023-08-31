use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::models::ApplicationState;
use async_mutex::Mutex as AsyncMutex;
use crate::node::db::meta_db::meta_db_view::{MetaPassStore, VaultStore};
use crate::node::db::events::common::VaultInfo;
use crate::node::logger::MetaLogger;
use crate::node::common::data_transfer::MpscDataTransfer;

use async_trait::async_trait;

use crate::models::MetaPasswordId;
use crate::node::app::meta_app::{EmptyMetaClient, MetaClient, MetaClientContext, RegisteredMetaClient};
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::server::server_app::ServerApp;

#[async_trait(? Send)]
pub trait AppManagerAsyncRunner<Repo: KvLogEventRepo, Logger: MetaLogger> {
    async fn run(self);
}

#[async_trait(? Send)]
pub trait StateUpdateManager {
    async fn update_state(&self, new_state: ApplicationState);
}

#[async_trait(? Send)]
pub trait ApplicationStateManagerApi {
    async fn sign_up(&mut self, vault_name: &str, device_name: &str);
    async fn cluster_distribution(&self, pass_id: &str, pass: &str);
    async fn recover(&self, meta_pass_id: MetaPasswordId);
}

pub struct ApplicationStateManager<Repo, Logger, StateManager>
    where
        Repo: KvLogEventRepo,
        Logger: MetaLogger,
        StateManager: StateUpdateManager {

    pub state_manager: Rc<StateManager>,

    pub meta_client: Rc<MetaClient<Repo, Logger>>,

    pub device_repo: Rc<Repo>,
    pub server: Rc<ServerApp<Repo, Logger>>,

    pub client_logger: Rc<Logger>,
    pub vd_logger: Rc<Logger>,

    pub data_transfer: Rc<MpscDataTransfer>,
}

#[async_trait(? Send)]
impl <Repo, Logger, State> ApplicationStateManagerApi for ApplicationStateManager <Repo, Logger, State>
    where
        Repo: KvLogEventRepo,
        Logger: MetaLogger,
        State: StateUpdateManager {

    async fn sign_up(&mut self, vault_name: &str, device_name: &str) {
        match self.meta_client.as_ref() {
            MetaClient::Empty(client) => {
                let new_client_result = client
                    .get_or_create_local_vault(vault_name, device_name)
                    .await;

                if let Ok(new_client) = new_client_result {
                    new_client.ctx.enable_join().await;
                    self.meta_client = Rc::new(MetaClient::Init(new_client));
                }
            }
            MetaClient::Init(client) => {
                //ignore
                self.meta_client = Rc::new(MetaClient::Registered(client.sign_up().await));
            }
            MetaClient::Registered(_) => {
                //ignore
            }
        };

        self.on_update().await;
    }

    async fn recover(&self, meta_pass_id: MetaPasswordId) {
        match self.meta_client.as_ref() {
            MetaClient::Empty(_) => {

            }
            MetaClient::Init(_) => {

            }
            MetaClient::Registered(client) => {
                client.recovery_request(meta_pass_id).await;
            }
        }
    }

    async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        match self.meta_client.as_ref() {
            MetaClient::Empty(_) => {
            }
            MetaClient::Init(_) => {
            }
            MetaClient::Registered(client) => {
                client.cluster_distribution(pass_id, pass).await;
                let passwords = {
                    let meta_db = client.ctx.meta_db.lock().await;
                    match &meta_db.meta_pass_store {
                        MetaPassStore::Store { passwords, .. } => {
                            passwords.clone()
                        }
                        _ => {
                            vec![]
                        }
                    }
                };

                {
                    let mut app_state = client.ctx.app_state.lock().await;
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
        State: StateUpdateManager {

    pub fn new(
        client_repo: Rc<Repo>, device_repo: Rc<Repo>,
        logger: Rc<Logger>, vd_logger: Rc<Logger>,
        server: Rc<ServerApp<Repo, Logger>>,
        data_transfer: Rc<MpscDataTransfer>,
        state: Rc<State>) -> ApplicationStateManager<Repo, Logger, State> {

        logger.info("New. Application State Manager");

        let app_state = {
            let state = ApplicationState {
                meta_vault: None,
                vault: None,
                meta_passwords: vec![],
                join_component: false,
            };
            Arc::new(AsyncMutex::new(state))
        };

        let ctx = MetaClientContext::new(app_state.clone(), client_repo.clone(), logger.clone());
        let meta_client = MetaClient::Empty(EmptyMetaClient {
            ctx: Rc::new(ctx),
            logger: logger.clone()
        });

        ApplicationStateManager {
            meta_client: Rc::new(meta_client),
            client_logger: logger,
            vd_logger,
            device_repo,
            server,
            data_transfer,
            state_manager: state
        }
    }

    pub async fn setup_meta_client(&mut self) {
        self.client_logger.info("Setup meta client");

        match self.meta_client.as_ref() {
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
                            let meta_db = client.ctx.meta_db.lock().await;
                            new_meta_client.ctx.update_vault(vault.clone()).await;
                            new_meta_client
                                .ctx
                                .update_meta_passwords(&meta_db.meta_pass_store.passwords())
                                .await;
                        }

                        let registered_client = MetaClient::Registered(new_meta_client);
                        self.meta_client = Rc::new(registered_client);
                    } else {
                        self.meta_client = Rc::new(MetaClient::Init(init_client));
                    }
                }
            }
            MetaClient::Init(client) => {
                let new_client = client.sign_up().await;
                if let VaultInfo::Member { vault } = &new_client.vault_info {
                    MetaClientContext::update_vault(&new_client.ctx, vault.clone()).await;
                }
                self.meta_client = Rc::new(MetaClient::Registered(new_client));
            }
            MetaClient::Registered(client) => {
                self.registered_client(client).await;
            }
        }
    }

    pub async fn run_client_gateway(&self, data_transfer_client: Rc<MpscDataTransfer>, ctx: Rc<MetaClientContext<Repo, Logger>>, repo: Rc<Repo>) {
        let gateway = SyncGateway::new(
            repo, data_transfer_client, String::from("client-gateway"), self.client_logger.clone()
        );

        loop {
            async_std::task::sleep(Duration::from_secs(1)).await;
            gateway.sync().await;

            let meta_db = ctx.meta_db.lock().await;
            match &meta_db.vault_store {
                VaultStore::Store { vault, .. } => {
                    gateway.send_shared_secrets(vault).await;
                }
                _ => {
                    //skip
                }
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
        self.state_manager.update_state(self.get_state().await).await
    }

    pub async fn get_state(&self) -> ApplicationState {
        let app_state = match self.meta_client.as_ref() {
            MetaClient::Empty(client) => {
                client.ctx.app_state.lock().await
            }
            MetaClient::Init(client) => {
                client.ctx.app_state.lock().await
            }
            MetaClient::Registered(client) => {
                client.ctx.app_state.lock().await
            }
        };

        self.client_logger.debug(format!("Current app state for js: {:?}", app_state).as_str());

        app_state.deref().clone()
    }
}