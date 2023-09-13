use std::cell::RefCell;
use std::ops::Deref;
use std::sync::Arc;
use async_trait::async_trait;

use crate::models::ApplicationState;
use crate::models::MetaPasswordId;
use crate::node::app::meta_app::{EmptyMetaClient, MetaClient, MetaClientContext, RegisteredMetaClient};
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::db::events::common::VaultInfo;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_service::MetaDbService;
use crate::node::db::meta_db::store::meta_pass_store::MetaPassStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::MetaLogger;
use async_mutex::Mutex as AsyncMutex;

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
    pub sync_gateway: Arc<SyncGateway<Repo, Logger>>
}

impl <Repo, Logger, State> ApplicationStateManager <Repo, Logger, State>
    where
        Repo: KvLogEventRepo,
        Logger: MetaLogger,
        State: JsAppStateManager,
{

    pub async fn sign_up(&self, vault_name: &str, device_name: &str) {
        let mut curr_meta_client = self.meta_client.lock().await;

        match curr_meta_client.deref() {
            MetaClient::Empty(client) => {
                let new_client_result = client
                    .get_or_create_local_vault(vault_name, device_name)
                    .await;

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
        let curr_meta_client = self.meta_client.lock().await;

        match curr_meta_client.deref() {
            MetaClient::Empty(_) => {
            }
            MetaClient::Init(_) => {
            }
            MetaClient::Registered(client) => {
                self.client_logger.info("Password, cluster distribution");

                client.cluster_distribution(pass_id, pass).await;
                let passwords = {
                    let pass_store = self.meta_db_service.get_meta_pass_store().await.unwrap();
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
        State: JsAppStateManager
{

    pub fn new(
        persistent_obj: Arc<PersistentObject<Repo, Logger>>,
        meta_db_service: Arc<MetaDbService<Repo, Logger>>,
        logger: Arc<Logger>,
        data_transfer: Arc<MpscDataTransfer>,
        state: Arc<State>,
        sync_gateway: Arc<SyncGateway<Repo, Logger>>
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
            sync_gateway
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