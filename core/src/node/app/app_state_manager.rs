use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::models::ApplicationState;
use crate::node::db::meta_db::store::meta_pass_store::MetaPassStore;
use crate::node::db::events::common::VaultInfo;
use crate::node::logger::MetaLogger;
use crate::node::common::data_transfer::MpscDataTransfer;

use async_trait::async_trait;

use crate::models::MetaPasswordId;
use crate::node::app::meta_app::{EmptyMetaClient, MetaClient, MetaClientContext, RegisteredMetaClient};
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_service::MetaDbService;
use crate::node::db::meta_db::store::vault_store::VaultStore;
use crate::node::server::server_app::ServerApp;

#[async_trait(? Send)]
pub trait StateUpdateManager {
    async fn update_state(&self, new_state: ApplicationState);
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

impl <Repo, Logger, State> ApplicationStateManager <Repo, Logger, State>
    where
        Repo: KvLogEventRepo,
        Logger: MetaLogger,
        State: StateUpdateManager {

    pub async fn sign_up(&mut self, vault_name: &str, device_name: &str) {
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

    pub async fn recover(&self, meta_pass_id: MetaPasswordId) {
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

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        match self.meta_client.as_ref() {
            MetaClient::Empty(_) => {
            }
            MetaClient::Init(_) => {
            }
            MetaClient::Registered(client) => {
                self.client_logger.info("Password, cluster distribution");

                client.cluster_distribution(pass_id, pass).await;
                let passwords = {
                    let pass_store = client.ctx.meta_db_service.get_meta_pass_store().await.unwrap();
                    match pass_store {
                        MetaPassStore::Store { passwords, .. } => {
                            passwords.clone()
                        }
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
        State: StateUpdateManager {

    pub fn new(
        client_repo: Rc<Repo>, device_repo: Rc<Repo>,
        logger: Rc<Logger>, vd_logger: Rc<Logger>,
        server: Rc<ServerApp<Repo, Logger>>,
        data_transfer: Rc<MpscDataTransfer>,
        state: Rc<State>, meta_db_service: Rc<MetaDbService<Repo, Logger>>) -> ApplicationStateManager<Repo, Logger, State> {

        logger.info("New. Application State Manager");

        let app_state = {
            let state = ApplicationState {
                meta_vault: None,
                vault: None,
                meta_passwords: vec![],
                join_component: false,
            };
            RefCell::new(state)
        };

        let ctx = MetaClientContext::new(app_state, client_repo.clone(), logger.clone(), meta_db_service);

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
                            new_meta_client.ctx.update_vault(vault.clone()).await;
                            let meta_pass_store = client.ctx.meta_db_service.get_meta_pass_store().await.unwrap();

                            new_meta_client
                                .ctx
                                .update_meta_passwords(meta_pass_store.passwords())
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

    pub async fn run_client_gateway(data_transfer_client: Rc<MpscDataTransfer>, ctx: Rc<MetaClientContext<Repo, Logger>>, repo: Rc<Repo>, client_logger: Rc<Logger>) {
        let gateway = SyncGateway::new(
            repo, data_transfer_client, String::from("client-gateway"), client_logger
        );

        loop {
            async_std::task::sleep(Duration::from_secs(1)).await;
            gateway.sync().await;

            let vault_store = ctx.meta_db_service.get_vault_store()
                .await
                .unwrap();

            match vault_store {
                VaultStore::Store { vault, .. } => {
                    gateway.send_shared_secrets(&vault).await;
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
                client.ctx.app_state.borrow()
            }
            MetaClient::Init(client) => {
                client.ctx.app_state.borrow()
            }
            MetaClient::Registered(client) => {
                client.ctx.app_state.borrow()
            }
        };

        self.client_logger.debug(format!("Current app state for js: {:?}", app_state).as_str());

        //let new_state_js = serde_wasm_bindgen::to_value(&new_state).unwrap();
        app_state.clone()
    }
}