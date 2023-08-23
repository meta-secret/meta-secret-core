use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use wasm_bindgen::prelude::wasm_bindgen;
use meta_secret_core::models::ApplicationState;
use async_mutex::Mutex as AsyncMutex;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use meta_secret_core::node::db::meta_db::meta_db_view::{MetaPassStore, VaultStore};
use meta_secret_core::node::db::events::common::VaultInfo;
use meta_secret_core::node::logger::MetaLogger;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use crate::commit_log::{WasmMetaLogger, WasmRepo};

use crate::{alert, JsAppState};
use crate::objects::ToJsValue;
use crate::virtual_device::VirtualDevice;
use crate::wasm_app::{EmptyMetaClient, MetaClientContext, RegisteredMetaClient, WasmMetaClient};
use crate::wasm_server::WasmServer;
use crate::wasm_sync_gateway::WasmSyncGateway;
use meta_secret_core::node::logger::LoggerId;

#[wasm_bindgen]
pub struct ApplicationStateManager {
    meta_client: Rc<WasmMetaClient>,
    js_app_state: JsAppState
}

#[wasm_bindgen]
impl ApplicationStateManager {
    pub fn new(js_app_state: JsAppState) -> ApplicationStateManager {

        let logger = Rc::new(WasmMetaLogger {
            id: LoggerId::Client
        });
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

        let client_repo = Rc::new(WasmRepo::default());
        let ctx = MetaClientContext::new(app_state.clone(), client_repo.clone(), logger.clone());
        let meta_client = WasmMetaClient::Empty(EmptyMetaClient {
            ctx: Rc::new(ctx),
            logger: logger.clone()
        });

        ApplicationStateManager {
            meta_client: Rc::new(meta_client),
            js_app_state
        }
    }

    pub async fn init(&mut self) {
        let client_logger = Rc::new(WasmMetaLogger {
            id: LoggerId::Client
        });
        client_logger.info("Initialize Application State Manager");

        client_logger.info("Init App State Manager");
        let data_transfer = Rc::new(MpscDataTransfer::new());

        ApplicationStateManager::run_server(&data_transfer);

        let vd1_logger = Rc::new(WasmMetaLogger {
          id: LoggerId::Vd1
        });
        let device_repo_1 = Rc::new(WasmRepo::virtual_device());
        VirtualDevice::setup_virtual_device(device_repo_1, data_transfer.clone(), vd1_logger);

        //let device_repo_2 = Rc::new(WasmRepo::virtual_device_2());
        //VirtualDevice::setup_virtual_device(device_repo_2, data_transfer.clone());

        self.setup_meta_client(client_logger.clone()).await;

        ApplicationStateManager::run_client_gateway(
            data_transfer.clone(),
            client_logger.clone(),
            self.meta_client.get_ctx()
        );

        self.on_update().await;
    }

    async fn setup_meta_client(&mut self, logger: Rc<WasmMetaLogger>) {
        logger.info("Setup meta client");

        match self.meta_client.as_ref() {
            WasmMetaClient::Empty(client) => {
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

                        let registered_client = WasmMetaClient::Registered(new_meta_client);
                        self.meta_client = Rc::new(registered_client);
                    } else {
                        self.meta_client = Rc::new(WasmMetaClient::Init(init_client));
                    }
                }
            }
            WasmMetaClient::Init(client) => {
                let new_client = client.sign_up().await;
                if let VaultInfo::Member { vault } = &new_client.vault_info {
                    MetaClientContext::update_vault(&new_client.ctx, vault.clone()).await;
                }
                self.meta_client = Rc::new(WasmMetaClient::Registered(new_client));
            }
            WasmMetaClient::Registered(client) => {
                self.registered_client(client).await;
            }
        }
    }

    fn run_client_gateway(data_transfer: Rc<MpscDataTransfer>, logger: Rc<dyn MetaLogger>, ctx: Rc<MetaClientContext>) {
        logger.info("Run client gateway");
        let data_transfer_client = data_transfer.clone();
        spawn_local(async move {
            let gateway = WasmSyncGateway::new(data_transfer_client, String::from("client-gateway"), logger);
            loop {
                async_std::task::sleep(Duration::from_secs(1)).await;
                gateway.sync().await;

                let meta_db = ctx.meta_db.lock().await;
                match &meta_db.vault_store {
                    VaultStore::Store { vault, .. } => {
                        gateway.sync_shared_secrets(vault, ).await;
                    }
                    _ => {
                        //skip
                    }
                }
            }
        });
    }

    fn run_server(data_transfer: &Rc<MpscDataTransfer>) {
        let data_transfer_server = data_transfer.clone();
        spawn_local(async move {
            let _ = WasmServer::run(data_transfer_server).await;
        });
    }

    async fn registered_client(&self, client: &RegisteredMetaClient) {
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

    pub async fn sign_up(&mut self, vault_name: &str, device_name: &str) {
        match self.meta_client.as_ref() {
            WasmMetaClient::Empty(client) => {
                let new_client_result = client
                    .get_or_create_local_vault(vault_name, device_name)
                    .await;

                if let Ok(new_client) = new_client_result {
                    new_client.ctx.enable_join().await;
                    self.meta_client = Rc::new(WasmMetaClient::Init(new_client));
                }
            }
            WasmMetaClient::Init(client) => {
                //ignore
                self.meta_client = Rc::new(WasmMetaClient::Registered(client.sign_up().await));
            }
            WasmMetaClient::Registered(_) => {
                //ignore
            }
        };

        self.on_update().await;
    }

    async fn on_update(&self) {
        // update app state in vue
        self.js_app_state.updateJsState();
    }

    pub async fn get_state(&self) -> JsValue {
        let logger = WasmMetaLogger {
            id: LoggerId::Client
        };

        let app_state = match self.meta_client.as_ref() {
            WasmMetaClient::Empty(client) => {
                client.ctx.app_state.lock().await
            }
            WasmMetaClient::Init(client) => {
                client.ctx.app_state.lock().await
            }
            WasmMetaClient::Registered(client) => {
                client.ctx.app_state.lock().await
            }
        };

        logger.debug(format!("Current app state for js: {:?}", app_state).as_str());

        app_state.to_js().unwrap()
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        match self.meta_client.as_ref() {
            WasmMetaClient::Empty(_) => {
            }
            WasmMetaClient::Init(_) => {
            }
            WasmMetaClient::Registered(client) => {
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
