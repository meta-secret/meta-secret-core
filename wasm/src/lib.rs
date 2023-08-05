extern crate core;

use std::rc::Rc;
use std::sync::{Arc};
use wasm_bindgen::prelude::*;
use meta_secret_core::models::{ApplicationState, MetaVault, UserCredentials, VaultDoc};
use crate::wasm_app::{EmptyMetaClient, InitMetaClient, MetaClientContext, RegisteredMetaClient, WasmMetaClient};
use meta_secret_core::node::app::meta_app::{MetaVaultManager, UserCredentialsManager};
use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::objects::ToJsValue;

use async_mutex::Mutex as AsyncMutex;
use meta_secret_core::node::db::commit_log::MetaDbManager;
use meta_secret_core::node::db::meta_db::{MetaDb, MetaPassStore};
use meta_secret_core::node::db::models::VaultInfo;
use crate::db::WasmDbError;
use crate::gateway::WasmSyncGateway;
use crate::virtual_device::{VirtualDevice, VirtualDeviceEvent};

use gloo_timers::callback::Interval;
use wasm_bindgen_futures::spawn_local;
//use gloo_timers::future::TimeoutFuture;
use async_std::future::timeout;

use std::sync::mpsc::channel;

mod commit_log;
mod db;
mod objects;
mod utils;
mod wasm_app;
mod gateway;
mod virtual_device;

/// Json utilities https://github.com/rustwasm/wasm-bindgen/blob/main/crates/js-sys/tests/wasm/JSON.rs

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    pub type JsAppState;

    #[wasm_bindgen(structural, method)]
    pub fn updateJsState(this: &JsAppState);
}

#[wasm_bindgen]
extern "C" {
    pub fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
    pub async fn idbGet(db_name: &str, store_name: &str, key: &str) -> JsValue;
    pub async fn idbSave(db_name: &str, store_name: &str, key: &str, value: JsValue);

    pub async fn idbFindAll(db_name: &str, store_name: &str) -> JsValue;
}

#[wasm_bindgen]
pub fn configure() {
    utils::set_panic_hook();
}

#[wasm_bindgen]
pub async fn get_meta_vault() -> Result<Option<JsValue>, JsValue> {
    objects::get_meta_vault().await
}

///https://rustwasm.github.io/wasm-bindgen/examples/closures.html
#[wasm_bindgen]
pub async fn recover() -> Result<JsValue, JsValue> {
    log("wasm recover!");

    /*
    server_api::claim_for_password_recovery(&recovery_request)
    */

    Ok(JsValue::null())
}

/// Sync local commit log with server
#[wasm_bindgen]
pub async fn sync() -> Result<JsValue, JsValue> {
    wasm_app::sync_shares().await
}

#[wasm_bindgen]
pub async fn membership(
    candidate_user_sig: JsValue,
    request_type: JsValue,
) -> Result<JsValue, JsValue> {
    wasm_app::membership(candidate_user_sig, request_type).await
}


/// https://rustwasm.github.io/docs/wasm-bindgen/reference/arbitrary-data-with-serde.html
#[wasm_bindgen]
pub fn split(pass: &str) -> Result<JsValue, JsValue> {
    wasm_app::split(pass)
}

#[wasm_bindgen]
pub fn restore_password(shares_json: JsValue) -> Result<JsValue, JsValue> {
    /*log("wasm: restore password, core functionality");

    let user_shares: Vec<UserShareDto> = serde_wasm_bindgen::from_value(shares_json)?;

    let plain_text = recover_from_shares(user_shares).map_err(JsError::from)?;
    Ok(JsValue::from_str(plain_text.text.as_str()))*/
    Ok(JsValue::null())
}

#[wasm_bindgen]
pub struct ApplicationStateManager {
    meta_client: Rc<WasmMetaClient>,
    js_app_state: JsAppState,
    virtual_device: Rc<VirtualDevice>,
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

        let virtual_device = Rc::new(VirtualDevice::new());

        /*spawn_local(async move{
            virtual_device.sync().await;
        });*/

        /*spawn_local(async move {
            log("Future Done");
        });*/

        let (tx, rx) = flume::unbounded();
        //spawn_local(tx.send_async(123).await);
        spawn_local(async move {
            //let virtual_device_2 = Rc::new(VirtualDevice::new());

            loop {
                //virtual_device.sync().await;
                tx.send_async(123).await;
                async_std::task::sleep(std::time::Duration::from_secs(1)).await;
                //TimeoutFuture::new(1_000).await;
            }
            //log(rx.recv().unwrap().to_string().as_str());
        });

        spawn_local(async move {
            log("Tadadadammm!!!!!!!!!!!!");
            while let Ok(val) = rx.recv_async().await {
                log(val.to_string().as_str());
            }
        });

        /*let vd = VirtualDevice::new();
        let interval = Interval::new(1_000, || {
            spawn_local({
                vd.sync()
            });
        });
        interval.forget();*/

        ApplicationStateManager {
            meta_client: Rc::new(meta_client),
            js_app_state,
            virtual_device,
        }
    }

    pub async fn init(&mut self) {
        let init_state_result = self.virtual_device
            .handle(VirtualDeviceEvent::Init)
            .await;

        match init_state_result {
            Ok(init_state) => {
                let registered_result = init_state
                    .handle(VirtualDeviceEvent::SignUp)
                    .await;

                if let Ok(registered_state) = registered_result {
                    self.virtual_device = Rc::new(registered_state);
                    self.virtual_device.sync().await;
                }
            }
            Err(_) => {
                log("ERROR!!!")
            }
        }

        match self.meta_client.as_ref() {
            WasmMetaClient::Empty(client) => {
                let init_client_result = client.find_user_creds().await;
                match init_client_result {
                    Ok(Some(init_client)) => {
                        init_client.ctx.update_meta_vault(init_client.creds.user_sig.vault.clone()).await;
                        self.meta_client = Rc::new(WasmMetaClient::Init(init_client));
                    }
                    _ => {
                        log("!!!!!!!!!!!!!!!!!!!!!!!!");
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

    pub async fn sign_up(&mut self, vault_name: &str, device_name: &str) {
        match self.meta_client.as_ref() {
            WasmMetaClient::Empty(client) => {
                let new_client_result = client.get_or_create_local_vault(vault_name, device_name).await;

                match new_client_result {
                    Ok(new_client) => {
                        self.meta_client = Rc::new(WasmMetaClient::Init(new_client))
                    }
                    Err(_) => {}
                }
            }
            WasmMetaClient::Init(_) => {
                //ignore
            }
            WasmMetaClient::Registered(_) => {
                //ignore
            }
        };

        //self.meta_client.sign_up(vault_name, device_name).await;
        //self.on_update().await;
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