use std::rc::Rc;
use std::sync::Arc;

use wasm_bindgen::prelude::wasm_bindgen;
use meta_secret_core::models::ApplicationState;
use async_mutex::Mutex as AsyncMutex;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use meta_secret_core::node::db::meta_db::MetaPassStore;
use meta_secret_core::node::db::models::VaultInfo;
use meta_secret_core::node::server::server_app::MpscDataTransfer;
use crate::commit_log::WasmRepo;

use crate::{JsAppState, log};
use crate::objects::ToJsValue;
use crate::virtual_device::VirtualDevice;
use crate::wasm_app::{EmptyMetaClient, MetaClientContext, RegisteredMetaClient, WasmMetaClient};
use crate::wasm_server::WasmServer;

#[wasm_bindgen]
pub struct ApplicationStateManager {
    meta_client: Rc<WasmMetaClient>,
    js_app_state: JsAppState,
}

#[wasm_bindgen]
impl ApplicationStateManager {
    pub fn new(js_app_state: JsAppState) -> ApplicationStateManager {
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
        let meta_client = WasmMetaClient::Empty(EmptyMetaClient {
            ctx: Rc::new(MetaClientContext::new(app_state.clone(), client_repo.clone()))
        });

        ApplicationStateManager {
            meta_client: Rc::new(meta_client),
            js_app_state,
        }
    }

    pub async fn init(&mut self) {
        log("wasm: Init App State Manager");
        let data_transfer = Rc::new(MpscDataTransfer::new());

        VirtualDevice::setup_virtual_device(data_transfer.clone());

        match self.meta_client.as_ref() {
            WasmMetaClient::Empty(client) => {
                let init_client_result = client.find_user_creds().await;
                match init_client_result {
                    Ok(Some(init_client)) => {
                        init_client.ctx.update_meta_vault(init_client.creds.user_sig.vault.clone()).await;
                        self.meta_client = Rc::new(WasmMetaClient::Init(init_client));

                        spawn_local(async move {
                            let _ = WasmServer::run(data_transfer).await;
                        });
                    }
                    _ => {
                        crate::log("!!!!!!!!!!!!!!!!!!!!!!!!");
                        spawn_local(async move {
                            let _ = WasmServer::run(data_transfer).await;
                        });
                    }
                }
            }
            WasmMetaClient::Init(client) => {
                let new_client = client.sign_up().await;
                self.meta_client = Rc::new(WasmMetaClient::Registered(new_client));
            }
            WasmMetaClient::Registered(client) => {
                self.registered_client(client).await;
            }
        }

        self.on_update().await;
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
                let new_client_result = client.get_or_create_local_vault(vault_name, device_name).await;

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

        app_state.to_js().unwrap()
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        match self.meta_client.as_ref() {
            WasmMetaClient::Empty(_) => {}
            WasmMetaClient::Init(_) => {}
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
