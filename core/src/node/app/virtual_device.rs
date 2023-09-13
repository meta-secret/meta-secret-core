use std::cell::RefCell;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::models::ApplicationState;
use crate::node::app::meta_app::{EmptyMetaClient, MetaClient, MetaClientContext};
use crate::node::app::meta_manager::UserCredentialsManager;
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::db::actions::join;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_service::MetaDbService;
use crate::node::db::meta_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::MetaLogger;

pub struct VirtualDevice<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub meta_client: MetaClient<Repo, Logger>,
    pub ctx: Arc<MetaClientContext>,
    pub data_transfer: Arc<MpscDataTransfer>,
    pub logger: Arc<Logger>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualDeviceEvent {
    Init,
    SignUp,
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> VirtualDevice<Repo, Logger> {
    pub fn new(
        persistent_object: Arc<PersistentObject<Repo, Logger>>,
        meta_db_service: Arc<MetaDbService<Repo, Logger>>,
        data_transfer: Arc<MpscDataTransfer>, logger: Arc<Logger>,
    ) -> VirtualDevice<Repo, Logger> {
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

            MetaClientContext { app_state }
        };
        let ctx = Arc::new(ctx);

        let empty_meta_client = EmptyMetaClient {
            ctx: ctx.clone(),
            logger: logger.clone(),
            persistent_obj: persistent_object,
            meta_db_service,
        };

        Self {
            meta_client: MetaClient::Empty(empty_meta_client),
            ctx,
            data_transfer,
            logger: logger.clone(),
        }
    }

    pub async fn event_handler(
        persistent_object: Arc<PersistentObject<Repo, Logger>>,
        meta_db_service: Arc<MetaDbService<Repo, Logger>>,
        data_transfer: Arc<MpscDataTransfer>, logger: Arc<Logger>,
    ) {
        logger.info("Run virtual device event handler");

        let mut virtual_device = {
            let vd = VirtualDevice::new(
                persistent_object.clone(), meta_db_service.clone(), data_transfer.clone(), logger.clone()
            );
            Arc::new(vd)
        };

        logger.info("Generate device creds");
        let _ = persistent_object
            .repo
            .get_or_generate_user_creds(String::from("q"), String::from("virtual-device"))
            .await;

        let gateway = SyncGateway::new(
            persistent_object.repo.clone(),
            meta_db_service.clone(),
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
                    virtual_device = Arc::new(registered_state);
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
                MetaClient::Empty(_) => {
                    meta_db_service.sync_db().await;
                }
                MetaClient::Init(client) => {
                    meta_db_service
                        .update_with_vault(client.creds.user_sig.vault.name.clone())
                        .await;

                    let vault_store = meta_db_service.get_vault_store()
                        .await
                        .unwrap();

                    if let VaultStore::Store { tail_id, vault, .. } = vault_store {
                        let latest_event = client.persistent_obj.repo
                            .find_one(&tail_id)
                            .await;

                        if let Ok(Some(GenericKvLogEvent::Vault(VaultObject::JoinRequest { event }))) = latest_event {
                            let accept_event = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                                event: join::accept_join_request(&event, &vault),
                            });

                            let _ = client.persistent_obj
                                .repo
                                .save_event(&accept_event)
                                .await;
                        }

                        gateway.send_shared_secrets(&vault).await;
                    };
                }
                MetaClient::Registered(client) => {
                    meta_db_service
                        .update_with_vault(client.creds.user_sig.vault.name.clone())
                        .await;

                    let vault_store = meta_db_service.get_vault_store()
                        .await
                        .unwrap();

                    if let VaultStore::Store { tail_id, vault, .. } = vault_store {
                        let latest_event = client.persistent_obj
                            .repo
                            .find_one(&tail_id)
                            .await;

                        if let Ok(Some(GenericKvLogEvent::Vault(VaultObject::JoinRequest { event }))) = latest_event {
                            let accept_event = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                                event: join::accept_join_request(&event, &vault),
                            });

                            let _ = client.persistent_obj
                                .repo
                                .save_event(&accept_event)
                                .await;
                        }

                        gateway.send_shared_secrets(&vault).await;
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
                    logger: self.logger.clone(),
                })
            }
            (MetaClient::Init(client), VirtualDeviceEvent::SignUp) => {
                Ok(VirtualDevice {
                    meta_client: MetaClient::Registered(client.sign_up().await),
                    ctx: client.ctx.clone(),
                    data_transfer: self.data_transfer.clone(),
                    logger: self.logger.clone(),
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
