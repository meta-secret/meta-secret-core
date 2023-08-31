use std::rc::Rc;
use std::sync::Arc;

use async_mutex::Mutex as AsyncMutex;
use serde::{Deserialize, Serialize};

use crate::models::ApplicationState;
use crate::node::app::meta_manager::UserCredentialsManager;
use crate::node::db::meta_db::meta_db_manager::MetaDbManager;
use crate::node::db::actions::join;
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::{KvLogEventRepo};
use crate::node::db::meta_db::meta_db_view::{MetaDb, VaultStore};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::MetaLogger;
use crate::node::common::data_transfer::MpscDataTransfer;

use crate::node::app::meta_app::{EmptyMetaClient, MetaClientContext, MetaClient};
use crate::node::app::sync_gateway::SyncGateway;

pub struct VirtualDevice<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub meta_client: MetaClient<Repo, Logger>,
    pub ctx: Rc<MetaClientContext<Repo, Logger>>,
    pub data_transfer: Rc<MpscDataTransfer>,
    pub logger: Rc<Logger>
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualDeviceEvent {
    Init,
    SignUp,
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> VirtualDevice<Repo, Logger> {
    pub fn new(virtual_device_repo: Rc<Repo>, data_transfer: Rc<MpscDataTransfer>, logger: Rc<Logger>) -> VirtualDevice<Repo, Logger> {
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
            let persistent_object = Rc::new(PersistentObject::new(virtual_device_repo.clone(), logger.clone()));

            let meta_db_manager = MetaDbManager {
                persistent_obj: persistent_object.clone(),
                repo: virtual_device_repo.clone(),
                logger: logger.clone(),
            };

            MetaClientContext {
                meta_db: Arc::new(AsyncMutex::new(MetaDb::new(String::from("virtual-device"), logger.clone()))),
                meta_db_manager,
                app_state,
                persistent_object: persistent_object.clone(),
                repo: virtual_device_repo,
                logger: logger.clone()
            }
        };
        let ctx = Rc::new(ctx);

        let empty_meta_client = EmptyMetaClient {
            ctx: ctx.clone(),
            logger: logger.clone()
        };

        Self {
            meta_client: MetaClient::Empty(empty_meta_client),
            ctx,
            data_transfer,
            logger: logger.clone(),
        }
    }

    pub async fn event_handler(device_repo: Rc<Repo>, data_transfer: Rc<MpscDataTransfer>, logger: Rc<Logger>) {
        logger.info("Run virtual device event handler");

        let mut virtual_device = {
            let vd = VirtualDevice::new(device_repo.clone(), data_transfer.clone(), logger.clone());
            Rc::new(vd)
        };

        logger.info("Generate device creds");
        let _ = device_repo
            .get_or_generate_user_creds(String::from("q"), String::from("virtual-device"))
            .await;

        let gateway = SyncGateway::new(
            virtual_device.ctx.repo.clone(),
            data_transfer.clone(),
            String::from("vd-gateway"),
            logger.clone()
        );

        let init_state_result = virtual_device
            .handle(VirtualDeviceEvent::Init)
            .await;

        match init_state_result {
            Ok(init_state) => {
                let registered_result = init_state
                    .handle(VirtualDeviceEvent::SignUp)
                    .await;

                if let Ok(registered_state) = registered_result {
                    virtual_device = Rc::new(registered_state);
                    gateway.sync().await;
                }
            }
            Err(_) => {
                logger.error("ERROR!!!")
            }
        }

        loop {
            async_std::task::sleep(std::time::Duration::from_secs(1)).await;

            gateway.sync().await;

            match &virtual_device.meta_client {
                MetaClient::Empty(client) => {
                    let meta_db_manager = &client.ctx.meta_db_manager;
                    let mut meta_db = client.ctx.meta_db.lock().await;
                    let _ = meta_db_manager.sync_meta_db(&mut meta_db).await;
                }
                MetaClient::Init(client) => {
                    let meta_db_manager = &client.ctx.meta_db_manager;
                    let mut meta_db = client.ctx.meta_db.lock().await;
                    meta_db.update_vault_info(client.creds.user_sig.vault.name.as_str());

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

                        gateway.send_shared_secrets(vault).await;
                    };
                }
                MetaClient::Registered(client) => {
                    let meta_db_manager = &client.ctx.meta_db_manager;
                    let mut meta_db = client.ctx.meta_db.lock().await;
                    meta_db.update_vault_info(client.creds.user_sig.vault.name.as_str());

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

                        gateway.send_shared_secrets(vault).await;
                    };
                }
            };
        }
    }


    pub async fn handle(&self, event: VirtualDeviceEvent) -> anyhow::Result<VirtualDevice<Repo, Logger>> {
        self.logger.info(format!("handle event: {:?}", event).as_str());

        match (&self.meta_client, &event) {
            (MetaClient::Empty(client), VirtualDeviceEvent::Init) => {
                // init

                let vault_name = "q";
                let device_name = "virtual-device";

                let init_client = client
                    .get_or_create_local_vault(vault_name, device_name)
                    .await?;

                Ok(VirtualDevice {
                    meta_client: MetaClient::Init(init_client),
                    ctx: client.ctx.clone(),
                    data_transfer: self.data_transfer.clone(),
                    logger: self.logger.clone()
                })
            }
            (MetaClient::Init(client), VirtualDeviceEvent::SignUp) => {
                Ok(VirtualDevice {
                    meta_client: MetaClient::Registered(client.sign_up().await),
                    ctx: client.ctx.clone(),
                    data_transfer: self.data_transfer.clone(),
                    logger: self.logger.clone()
                })
            }
            _ => {
                let msg = format!(
                    "Invalid state!!!!!!!!!!!!!!!: state: {:?}, event: {:?}",
                    self.meta_client.to_string(),
                    &event
                );
                self.logger.info(msg.as_str());
                panic!("Invalid state")
            }
        }
    }
}
