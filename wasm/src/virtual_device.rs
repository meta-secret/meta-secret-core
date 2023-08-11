use std::rc::Rc;
use std::sync::Arc;

use async_mutex::Mutex as AsyncMutex;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

use meta_secret_core::models::ApplicationState;
use meta_secret_core::node::app::meta_app::UserCredentialsManager;
use meta_secret_core::node::db::commit_log::MetaDbManager;
use meta_secret_core::node::db::events::join;
use meta_secret_core::node::db::generic_db::{FindOneQuery, SaveCommand};
use meta_secret_core::node::db::meta_db::{MetaDb, VaultStore};
use meta_secret_core::node::db::models::{GenericKvLogEvent, VaultObject};
use meta_secret_core::node::db::persistent_object::PersistentObject;
use meta_secret_core::node::server::server_app::MpscDataTransfer;

use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::db::WasmDbError;
use crate::{alert, log};
use crate::wasm_app::{EmptyMetaClient, MetaClientContext, WasmMetaClient};
use crate::wasm_sync_gateway::WasmSyncGateway;

pub struct VirtualDevice {
    pub meta_client: WasmMetaClient,
    pub ctx: Rc<MetaClientContext>,
    pub data_transfer: Rc<MpscDataTransfer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualDeviceEvent {
    Init,
    SignUp,
}

impl VirtualDevice {
    pub fn new(data_transfer: Rc<MpscDataTransfer>) -> VirtualDevice {
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
            let virtual_device_repo = Rc::new(WasmRepo::virtual_device());

            let logger = Rc::new(WasmMetaLogger {});
            let persistent_object = Rc::new(PersistentObject::new(virtual_device_repo.clone(), logger.clone()));

            let meta_db_manager = MetaDbManager {
                persistent_obj: persistent_object.clone(),
                repo: virtual_device_repo.clone(),
                logger,
            };

            MetaClientContext {
                meta_db: Arc::new(AsyncMutex::new(MetaDb::default())),
                meta_db_manager,
                app_state,
                persistent_object: persistent_object.clone(),
                repo: virtual_device_repo,
            }
        };
        let ctx = Rc::new(ctx);

        let empty_meta_client = EmptyMetaClient {
            ctx: ctx.clone()
        };

        Self {
            meta_client: WasmMetaClient::Empty(empty_meta_client),
            ctx,
            data_transfer,
        }
    }

    pub fn setup_virtual_device(data_transfer: Rc<MpscDataTransfer>) {
        log("wasm: Setup virtual device");
        spawn_local(async move {
            Self::event_handler(data_transfer).await;
        });
    }

    async fn event_handler(data_transfer: Rc<MpscDataTransfer>) {
        log("wasm: run virtual device event handler");

        let mut virtual_device = Rc::new(VirtualDevice::new(data_transfer.clone()));

        let device_repo = Rc::new(WasmRepo::virtual_device());

        log("wasm: generate device creds");
        let _ = device_repo
            .get_or_generate_user_creds(String::from("q"), String::from("virtual-device"))
            .await;

        let gateway = WasmSyncGateway::new_with_custom_repo(virtual_device.ctx.repo.clone(), data_transfer.clone());

        let init_state_result = virtual_device
            .handle(VirtualDeviceEvent::Init, &gateway)
            .await;

        match init_state_result {
            Ok(init_state) => {
                let registered_result = init_state
                    .handle(VirtualDeviceEvent::SignUp, &gateway)
                    .await;

                if let Ok(registered_state) = registered_result {
                    virtual_device = Rc::new(registered_state);
                    gateway.sync().await;
                }
            }
            Err(_) => {
                log("ERROR!!!")
            }
        }

        loop {
            async_std::task::sleep(std::time::Duration::from_secs(1)).await;

            gateway.sync().await;

            match &virtual_device.meta_client {
                WasmMetaClient::Empty(client) => {
                    let meta_db_manager = &client.ctx.meta_db_manager;
                    let mut meta_db = client.ctx.meta_db.lock().await;
                    let _ = meta_db_manager.sync_meta_db(&mut meta_db).await;
                }
                WasmMetaClient::Init(client) => {
                    let meta_db_manager = &client.ctx.meta_db_manager;
                    let mut meta_db = client.ctx.meta_db.lock().await;
                    let _ = meta_db_manager.sync_meta_db(&mut meta_db).await;

                    if let VaultStore::Store { tail_id, vault, .. } = &meta_db.vault_store {
                        let latest_event = meta_db_manager
                            .persistent_obj
                            .repo
                            .find_one(tail_id).await;

                        if let Ok(Some(GenericKvLogEvent::Vault(VaultObject::JoinRequest { event }))) = latest_event {
                            let accept_event = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                                event: join::accept_join_request(&event, vault),
                            });

                            let _ = meta_db_manager
                                .persistent_obj
                                .repo
                                .save_event(&accept_event)
                                .await;
                        }
                    };
                }
                WasmMetaClient::Registered(client) => {
                    let meta_db_manager = &client.ctx.meta_db_manager;
                    let mut meta_db = client.ctx.meta_db.lock().await;
                    let _ = meta_db_manager.sync_meta_db(&mut meta_db).await;

                    if let VaultStore::Store { tail_id, vault, .. } = &meta_db.vault_store {
                        let latest_event = meta_db_manager
                            .persistent_obj
                            .repo
                            .find_one(tail_id).await;

                        if let Ok(Some(GenericKvLogEvent::Vault(VaultObject::JoinRequest { event }))) = latest_event {
                            let accept_event = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                                event: join::accept_join_request(&event, vault),
                            });

                            let _ = meta_db_manager
                                .persistent_obj
                                .repo
                                .save_event(&accept_event)
                                .await;
                        }
                    };
                }
            };
        }
    }


    pub async fn handle(&self, event: VirtualDeviceEvent, gateway: &WasmSyncGateway) -> Result<VirtualDevice, WasmDbError> {
        log("wasm: handle");
        gateway.sync().await;

        match (&self.meta_client, &event) {
            (WasmMetaClient::Empty(client), VirtualDeviceEvent::Init) => {
                // init

                let vault_name = "q";
                let device_name = "virtual-device";

                let init_client = client
                    .get_or_create_local_vault(vault_name, device_name)
                    .await?;

                Ok(VirtualDevice {
                    meta_client: WasmMetaClient::Init(init_client),
                    ctx: client.ctx.clone(),
                    data_transfer: self.data_transfer.clone(),
                })
            }
            (WasmMetaClient::Init(client), VirtualDeviceEvent::SignUp) => {
                Ok(VirtualDevice {
                    meta_client: WasmMetaClient::Registered(client.sign_up().await),
                    ctx: client.ctx.clone(),
                    data_transfer: self.data_transfer.clone(),
                })
            }
            _ => {
                log(format!("Invalid state!!!!!!!!!!!!!!!: state: {:?}, event: {:?}", self.meta_client.to_string(), &event).as_str());
                panic!("Invalid state")
            }
        }
    }
}
