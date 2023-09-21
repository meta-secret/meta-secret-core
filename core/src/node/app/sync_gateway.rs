use std::sync::Arc;
use std::time::Duration;

use crate::models::VaultDoc;
use crate::node::app::meta_vault_manager::UserCredentialsManager;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::db::events::common::{LogEventKeyBasedRecord, ObjectCreator, SharedSecretObject};
use crate::node::db::events::db_tail::{DbTail, DbTailObject};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::KvLogEventLocal;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, ObjectId};
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_service::MetaDbService;
use crate::node::db::meta_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::MetaLogger;
use crate::node::server::data_sync::DataSyncMessage;
use crate::node::server::request::SyncRequest;

pub struct SyncGateway<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub id: String,
    pub logger: Arc<Logger>,
    pub repo: Arc<Repo>,
    pub persistent_object: Arc<PersistentObject<Repo, Logger>>,
    pub data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
    pub meta_db_service: Arc<MetaDbService<Repo, Logger>>,
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> SyncGateway<Repo, Logger> {
    pub fn new(
        repo: Arc<Repo>,
        meta_db_service: Arc<MetaDbService<Repo, Logger>>,
        data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
        gateway_id: String,
        logger: Arc<Logger>,
    ) -> SyncGateway<Repo, Logger> {
        logger.info("Create new wasm sync gateway instance");

        let persistent_object = {
            let obj = PersistentObject::new(repo.clone(), logger.clone());
            Arc::new(obj)
        };

        SyncGateway {
            id: gateway_id,
            logger,
            repo,
            persistent_object,
            data_transfer: data_transfer.clone(),
            meta_db_service,
        }
    }

    pub async fn run(&self) {
        loop {
            async_std::task::sleep(Duration::from_secs(1)).await;
            self.sync().await;

            let vault_store = self.meta_db_service.get_vault_store().await.unwrap();

            match vault_store {
                VaultStore::Store { vault, .. } => {
                    self.send_shared_secrets(&vault).await;
                }
                _ => {
                    //skip
                }
            }
        }
    }

    pub async fn send_shared_secrets(&self, vault_doc: &VaultDoc) {
        let creds_result = self.repo.find_user_creds().await;

        if let Ok(Some(client_creds)) = creds_result {
            let user_pk = client_creds.user_sig.public_key.base64_text;

            for user_sig in &vault_doc.signatures {
                if user_pk == user_sig.public_key.base64_text.clone() {
                    continue;
                }

                let obj_desc = ObjectDescriptor::SharedSecret {
                    vault_name: user_sig.vault.name.clone(),
                    device_id: user_sig.vault.device.device_id.clone(),
                };
                let obj_id = ObjectId::unit(&obj_desc);
                let events = self.persistent_object.find_object_events(&obj_id).await;

                for event in events {
                    self.logger
                        .debug(format!("Send shared secret event to server: {:?}", event).as_str());
                    self.data_transfer.send_to_service(DataSyncMessage::Event(event)).await;
                }
            }
        }
    }

    pub async fn sync(&self) {
        let creds_result = self.repo.find_user_creds().await;

        match creds_result {
            Err(_) => {
                self.logger
                    .debug(format!("Gw type: {:?}, Error. User credentials db error. Skip", self.id).as_str());
                //skip
            }

            Ok(None) => {
                self.logger
                    .debug(format!("Gw type: {:?}, Error. Empty user credentials. Skip", self.id).as_str());
                //skip
            }

            Ok(Some(client_creds)) => {
                let vault_name = client_creds.user_sig.vault.name.as_str();
                let db_tail_result = self.persistent_object.get_db_tail(vault_name).await;

                match db_tail_result {
                    Ok(db_tail) => {
                        let new_tail_for_gi = self.get_new_tail_for_global_index(&db_tail).await;

                        let new_tail_for_vault = self.get_new_tail_for_an_obj(&db_tail.vault_id).await;

                        let new_tail_for_meta_pass = self.get_new_tail_for_an_obj(&db_tail.meta_pass_id).await;

                        let new_tail_for_mem_pool = self.get_new_tail_for_mem_pool(&db_tail).await;

                        let new_db_tail = DbTail {
                            vault_id: new_tail_for_vault,
                            meta_pass_id: new_tail_for_meta_pass,

                            maybe_global_index_id: new_tail_for_gi,
                            maybe_mem_pool_id: new_tail_for_mem_pool.clone(),
                        };

                        self.save_updated_db_tail(db_tail, new_db_tail.clone()).await;

                        let sync_request = {
                            let vault_id_request = match &new_db_tail.vault_id {
                                DbTailObject::Empty { unit_id } => unit_id.clone(),
                                DbTailObject::Id { tail_id } => tail_id.next(),
                            };

                            let meta_pass_id_request = match &new_db_tail.meta_pass_id {
                                DbTailObject::Empty { unit_id } => unit_id.clone(),
                                DbTailObject::Id { tail_id } => tail_id.next(),
                            };

                            SyncRequest {
                                sender: client_creds.user_sig.as_ref().clone(),
                                global_index: new_db_tail.maybe_global_index_id.clone().map(|gi| gi.next()),
                                vault_tail_id: Some(vault_id_request),
                                meta_pass_tail_id: Some(meta_pass_id_request),
                            }
                        };

                        let mut latest_gi = new_db_tail.maybe_global_index_id.clone();
                        let mut latest_vault_id = new_db_tail.vault_id.clone();
                        let mut latest_meta_pass_id = new_db_tail.meta_pass_id.clone();

                        let new_events_res = self
                            .data_transfer
                            .send_to_service_and_get(DataSyncMessage::SyncRequest(sync_request))
                            .await;

                        match new_events_res {
                            Ok(new_events) => {
                                let log_msg = format!("id: {:?}. Sync gateway. New events: {:?}", self.id, new_events);
                                self.logger.debug(log_msg.as_str());

                                for new_event in new_events {
                                    let key = if let GenericKvLogEvent::SharedSecret(sss_obj) = &new_event {
                                        match sss_obj {
                                            SharedSecretObject::Split { event } => {
                                                let local_event =
                                                    GenericKvLogEvent::LocalEvent(KvLogEventLocal::LocalSecretShare {
                                                        event: event.clone(),
                                                    });

                                                let obj_desc = ObjectDescriptor::LocalSecretShare {
                                                    meta_pass_id: event.value.meta_password.meta_password.id.id.clone(),
                                                };
                                                let obj_id = ObjectId::unit(&obj_desc);
                                                let _ = self.repo.save(&obj_id, &local_event).await;
                                                obj_id
                                            }

                                            SharedSecretObject::RecoveryRequest { event } => {
                                                let obj_desc = event.key.obj_desc();
                                                let slot_id = self
                                                    .persistent_object
                                                    .find_tail_id_by_obj_desc(&obj_desc)
                                                    .await
                                                    .map(|id| id.next())
                                                    .unwrap_or(ObjectId::unit(&obj_desc));

                                                let _ = self.repo.save(&slot_id, &new_event).await;

                                                slot_id
                                            }

                                            SharedSecretObject::Recover { event } => {
                                                let obj_desc = event.key.obj_desc();
                                                let slot_id = self
                                                    .persistent_object
                                                    .find_tail_id_by_obj_desc(&obj_desc)
                                                    .await
                                                    .map(|id| id.next())
                                                    .unwrap_or(ObjectId::unit(&obj_desc));

                                                let _ = self.repo.save(&slot_id, &new_event).await;

                                                slot_id
                                            }
                                        }
                                    } else {
                                        self.repo
                                            .save_event(&new_event)
                                            .await
                                            .expect("Error saving secret share")
                                    };

                                    match new_event {
                                        GenericKvLogEvent::GlobalIndex(_) => latest_gi = Some(key),
                                        GenericKvLogEvent::Vault(_) => {
                                            latest_vault_id = DbTailObject::Id { tail_id: key.clone() }
                                        }
                                        GenericKvLogEvent::MetaPass(_) => {
                                            latest_meta_pass_id = DbTailObject::Id { tail_id: key.clone() }
                                        }
                                        _ => {
                                            //ignore any non global event
                                        }
                                    }
                                }

                                let latest_db_tail = DbTail {
                                    vault_id: latest_vault_id,
                                    meta_pass_id: latest_meta_pass_id,

                                    maybe_global_index_id: latest_gi,
                                    maybe_mem_pool_id: new_tail_for_mem_pool,
                                };

                                self.save_updated_db_tail(new_db_tail.clone(), latest_db_tail).await
                            }
                            Err(_err) => {
                                self.logger.error("DataSync error. Error loading events");
                                panic!("Error");
                            }
                        }
                    }
                    Err(_) => {
                        self.logger.error("Error! Db tail not exists");
                        panic!("Error");
                    }
                }
            }
        }
    }

    async fn save_updated_db_tail(&self, db_tail: DbTail, new_db_tail: DbTail) {
        if new_db_tail == db_tail {
            return;
        }

        //update db_tail
        let new_db_tail_event = GenericKvLogEvent::LocalEvent(KvLogEventLocal::DbTail {
            event: Box::new(KvLogEvent {
                key: KvKey::unit(&ObjectDescriptor::DbTail),
                value: new_db_tail.clone(),
            }),
        });

        let saved_event_res = self.repo.save_event(&new_db_tail_event).await;

        match saved_event_res {
            Ok(_) => self.logger.info("New db tail saved"),
            Err(_) => {
                self.logger.info("Error saving db tail");
            }
        };
    }

    async fn get_new_tail_for_an_obj(&self, db_tail_obj: &DbTailObject) -> DbTailObject {
        match db_tail_obj {
            DbTailObject::Empty { unit_id } => {
                let maybe_tail_id = self.persistent_object.find_tail_id(unit_id).await;
                match maybe_tail_id {
                    None => DbTailObject::Empty {
                        unit_id: unit_id.clone(),
                    },
                    Some(tail_id) => DbTailObject::Id { tail_id },
                }
            }
            DbTailObject::Id { tail_id } => {
                let tail_id_sync = match tail_id {
                    ObjectId::Unit { .. } => tail_id.clone(),
                    _ => tail_id.next(),
                };

                let obj_events = self.persistent_object.find_object_events(&tail_id_sync).await;
                let last_vault_event = obj_events.last().cloned();

                for client_event in obj_events {
                    self.logger.debug(
                        format!(
                            "Send event to server. May stuck if server won't response!!! : {:?}",
                            client_event
                        )
                        .as_str(),
                    );
                    self.data_transfer
                        .send_to_service(DataSyncMessage::Event(client_event))
                        .await;
                }

                let new_tail_id = last_vault_event
                    .map(|event| match event.key() {
                        KvKey::Empty { .. } => {
                            panic!("Invalid event");
                        }
                        KvKey::Key { obj_id, .. } => obj_id.clone(),
                    })
                    .unwrap_or(tail_id.clone());
                DbTailObject::Id { tail_id: new_tail_id }
            }
        }
    }

    async fn get_new_tail_for_global_index(&self, db_tail: &DbTail) -> Option<ObjectId> {
        let global_index = db_tail
            .maybe_global_index_id
            .clone()
            .unwrap_or(ObjectId::global_index_unit());

        self.persistent_object.find_tail_id(&global_index).await
    }

    async fn get_new_tail_for_mem_pool(&self, db_tail: &DbTail) -> Option<ObjectId> {
        let mem_pool_id = match db_tail.maybe_mem_pool_id.clone() {
            None => ObjectId::mempool_unit(),
            Some(obj_id) => obj_id.next(),
        };

        let mem_pool_events = self.persistent_object.find_object_events(&mem_pool_id).await;
        let last_pool_event = mem_pool_events.last().cloned();

        for client_event in mem_pool_events {
            self.logger
                .debug(format!("send mem pool request to server: {:?}", client_event).as_str());
            self.data_transfer
                .send_to_service(DataSyncMessage::Event(client_event))
                .await;
        }

        match last_pool_event {
            None => db_tail.maybe_mem_pool_id.clone(),
            Some(event) => match event.key() {
                KvKey::Empty { .. } => None,
                KvKey::Key { obj_id, .. } => Some(obj_id.clone()),
            },
        }
    }
}
