use std::rc::Rc;
use std::sync::Arc;
use meta_secret_core::models::ApplicationState;
use crate::wasm_app::{EmptyMetaClient, MetaClientContext, WasmMetaClient};
use async_mutex::Mutex as AsyncMutex;
use serde::{Deserialize, Serialize};
use meta_secret_core::node::db::commit_log::MetaDbManager;
use meta_secret_core::node::db::meta_db::MetaDb;
use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::db::WasmDbError;
use crate::gateway::WasmSyncGateway;
use crate::{log, wasm_app};

pub struct VirtualDevice {
    pub meta_client: WasmMetaClient
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualDeviceEvent {
    Init,
    SignUp
}

impl VirtualDevice {

    pub async fn handle(&self, event: VirtualDeviceEvent) -> Result<VirtualDevice, WasmDbError> {
        self.sync().await;

        match (&self.meta_client, &event) {
            (WasmMetaClient::Empty(client), VirtualDeviceEvent::Init) => {
                // init
                let vault_name = "q";
                let device_name = "virtual-device";

                let init_client = client
                    .get_or_create_local_vault(vault_name, device_name)
                    .await?;

                Ok(VirtualDevice {
                    meta_client: WasmMetaClient::Init(init_client)
                })
            }
            (WasmMetaClient::Init(client), VirtualDeviceEvent::SignUp) => {
                Ok(VirtualDevice {
                    meta_client: WasmMetaClient::Registered(client.sign_up().await)
                })
            }
            _ => {
                //log(format!("Invalid state!!!!!!!!!!!!!!!: state: {:?}, event: {:?}", this.meta_client.to_string(), &event).as_str());
                panic!("Invalid state")
            }
        }
    }

    pub fn new() -> VirtualDevice {
        let app_state = {
            let state = ApplicationState {
                meta_vault: None,
                vault: None,
                meta_passwords: vec![],
                join_component: false,
            };
            Arc::new(AsyncMutex::new(state))
        };

        let ctx = {
            let virtual_device_repo = {
                let repo = WasmRepo {
                    db_name: "meta-secret-v-device".to_string(),
                    store_name: "commit_log".to_string(),
                };
                Rc::new(repo)
            };

            let persistent_object = {
                let obj = wasm_app::get_persistent_object(virtual_device_repo.clone());
                Rc::new(obj)
            };

            let meta_db_manager = MetaDbManager {
                persistent_obj: persistent_object.clone(),
                repo: virtual_device_repo.clone(),
                logger: WasmMetaLogger {},
            };

            let v_device_sync_gateway = Rc::new(WasmSyncGateway::new_with_virtual_device(virtual_device_repo.clone()));

            MetaClientContext {
                meta_db: Arc::new(AsyncMutex::new(MetaDb::default())),
                meta_db_manager,
                app_state,
                sync_gateway: v_device_sync_gateway,
                persistent_object: persistent_object.clone(),
                repo: virtual_device_repo,
            }
        };

        let empty_meta_client = EmptyMetaClient {
            ctx: Rc::new(ctx)
        };

        Self {
            meta_client: WasmMetaClient::Empty(empty_meta_client)
        }
    }

    pub async fn sync(&self) {
        let sync_gateway = match &self.meta_client {
            WasmMetaClient::Empty(client) => {
                &client.ctx.sync_gateway
            }
            WasmMetaClient::Init(client) => {
                &client.ctx.sync_gateway
            }
            WasmMetaClient::Registered(client) => {
                &client.ctx.sync_gateway
            }
        };

        sync_gateway.sync().await;
    }
}
