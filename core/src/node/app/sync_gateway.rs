use std::rc::Rc;

use crate::node::app::meta_app::UserCredentialsManager;
use crate::node::db::events::common::{LogEventKeyBasedRecord, ObjectCreator, PublicKeyRecord};
use crate::node::db::events::db_tail::{DbTail, DbTailObject};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::KvLogEventLocal;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, ObjectId};

use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::data_sync::{DataSyncMessage, MetaLogger};
use crate::node::server::request::SyncRequest;
use crate::node::server::server_app::MpscSender;

pub struct SyncGateway {
    pub id: String,
    pub logger: Rc<dyn MetaLogger>,
    pub repo: Rc<dyn UserCredentialsManager>,
    pub persistent_object: Rc<PersistentObject>,
    pub data_transfer: Rc<MpscSender>,
}

impl SyncGateway {
    pub async fn sync(&self) {
        let creds_result = self.repo.find_user_creds().await;

        match creds_result {
            Err(_) => {
                self.logger
                    .log(format!("Gw type: {:?}, Error. User credentials db error. Skip", self.id).as_str());
                //skip
            }

            Ok(None) => {
                self.logger
                    .log(format!("Gw type: {:?}, Error. Empty user credentials. Skip", self.id).as_str());
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

                        self.logger.log("Create new db tail");
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
                                sender_pk: PublicKeyRecord {
                                    pk: client_creds.user_sig.public_key.as_ref().clone(),
                                },
                                global_index: new_db_tail.maybe_global_index_id.clone().map(|gi| gi.next()),
                                vault_tail_id: Some(vault_id_request),
                                meta_pass_tail_id: Some(meta_pass_id_request),
                            }
                        };

                        let mut latest_gi = new_db_tail.maybe_global_index_id.clone();
                        let mut latest_vault_id = new_db_tail.vault_id.clone();
                        let mut latest_meta_pass_id = new_db_tail.meta_pass_id.clone();

                        self.logger
                            .log(format!("id: {:?}.sync events!!!!!!!!!!!", self.id).as_str());
                        let new_events_res = self
                            .data_transfer
                            .send_and_get(DataSyncMessage::SyncRequest(sync_request))
                            .await;

                        match new_events_res {
                            Ok(new_events) => {
                                self.logger
                                    .log(format!("id: {:?}. New events: {:?}", self.id, new_events).as_str());

                                for new_event in new_events {
                                    let save_op = self.repo.save_event(&new_event).await;

                                    match save_op {
                                        Ok(()) => {
                                            match new_event {
                                                GenericKvLogEvent::GlobalIndex(_) => {
                                                    latest_gi = Some(new_event.key().obj_id.clone())
                                                }
                                                GenericKvLogEvent::Vault(_) => {
                                                    latest_vault_id = DbTailObject::Id {
                                                        tail_id: new_event.key().obj_id.clone(),
                                                    }
                                                }
                                                GenericKvLogEvent::MetaPass(_) => {
                                                    latest_meta_pass_id = DbTailObject::Id {
                                                        tail_id: new_event.key().obj_id.clone(),
                                                    }
                                                }
                                                _ => {
                                                    //ignore any non global event
                                                }
                                            }
                                        }
                                        Err(_) => {
                                            self.logger.log("Error saving new events to local db");
                                            panic!("Error");
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
                                self.logger.log("DataSync error. Error loading events");
                                panic!("Error");
                            }
                        }
                    }
                    Err(_) => {
                        self.logger.log("Error! Db tail not exists");
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
            Ok(()) => self.logger.log("New db tail saved"),
            Err(_) => {
                self.logger.log("Error saving db tail");
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
                    self.logger
                        .log(format!("send event to server. May stuck!!! : {:?}", client_event).as_str());
                    self.data_transfer.just_send(DataSyncMessage::Event(client_event)).await;
                    self.logger.log("AFTER SEND!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                }

                let new_tail_id = last_vault_event
                    .map(|event| event.key().obj_id.clone())
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
                .log(format!("send mem pool request to server: {:?}", client_event).as_str());
            self.data_transfer.just_send(DataSyncMessage::Event(client_event)).await;
        }

        match last_pool_event {
            None => db_tail.maybe_mem_pool_id.clone(),
            Some(event) => Some(event.key().obj_id.clone()),
        }
    }
}
