use std::marker::PhantomData;
use std::rc::Rc;

use wasm_bindgen::prelude::*;

use meta_secret_core::crypto::keys::KeyManager;
use meta_secret_core::models::UserCredentials;
use meta_secret_core::node::app::meta_app::{MetaVaultManager, UserCredentialsManager};
use meta_secret_core::node::db::events::object_id::{IdGen, ObjectId};
use meta_secret_core::node::db::events::sign_up::SignUpRequest;
use meta_secret_core::node::db::generic_db::SaveCommand;
use meta_secret_core::node::db::models::{DbTail, DbTailObject, GenericKvLogEvent, KvKey, KvLogEvent, KvLogEventLocal, LogEventKeyBasedRecord, ObjectCreator, ObjectDescriptor, PublicKeyRecord};
use meta_secret_core::node::server::data_sync::{DataSyncApi, MetaLogger};
use meta_secret_core::node::server::persistent_object::{PersistentGlobalIndex, PersistentObject};
use meta_secret_core::node::server::request::SyncRequest;

use crate::{alert, log, utils};
use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::db::WasmDbError;
use crate::wasm_app::get_data_sync;

#[wasm_bindgen]
pub struct WasmMetaServer {}

#[wasm_bindgen]
impl WasmMetaServer {
    pub fn new() -> WasmMetaServer {
        WasmMetaServer {}
    }

    #[wasm_bindgen]
    pub async fn run_server(&self) {
        let logger = WasmMetaLogger {};
        //log("WasmMetaServer::run_server");

        let server_repo = WasmRepo::server();
        let server_repo_rc = Rc::new(server_repo);

        let client_repo = WasmRepo::default();
        let client_repo_rc = Rc::new(client_repo);

        let client_persistent_object = PersistentObject {
            repo: client_repo_rc.clone(),
            global_index: PersistentGlobalIndex {
                repo: client_repo_rc.clone(),
                _phantom: PhantomData,
            },
        };

        let client_creds_result = client_repo_rc.find_user_creds()
            .await;

        let server_creds_result = server_repo_rc.find_user_creds()
            .await;

        match (server_creds_result, client_creds_result) {
            (Ok(None), _) => {
                self.generate_server_user_credentials(server_repo_rc).await;
            }

            (Err(_), _) => {
                self.generate_server_user_credentials(server_repo_rc).await;
            }

            (_, Err(_)) => {
                logger.log("Empty client credentials. Skip");
            }

            (_, Ok(None)) => {
                logger.log("Empty client credentials. Skip");
            }

            (Ok(Some(server_creds)), Ok(Some(client_creds))) => {
                //log("Wasm::run_server()");

                let vault_name = client_creds.user_sig.vault.name.as_str();
                let db_tail_result = client_persistent_object
                    .get_db_tail(vault_name)
                    .await;

                match db_tail_result {
                    Ok(db_tail) => {
                        let new_tail_for_gi = self.get_new_tail_for_global_index(
                            &client_persistent_object, &db_tail,
                        ).await;

                        let new_tail_for_vault = self.get_new_tail_for_an_obj(
                            &logger,
                            &server_repo_rc,
                            &client_persistent_object,
                            &server_creds,
                            &db_tail.vault_id,
                        ).await;

                        let new_tail_for_meta_pass = self.get_new_tail_for_an_obj(
                            &logger,
                            &server_repo_rc,
                            &client_persistent_object,
                            &server_creds,
                            &db_tail.meta_pass_id,
                        ).await;

                        let new_tail_for_mem_pool = self.get_new_tail_for_mem_pool(
                            &client_persistent_object,
                            &db_tail,
                            &logger,
                            &server_repo_rc,
                            &server_creds,
                        ).await;

                        let new_db_tail = DbTail {
                            vault_id: new_tail_for_vault,
                            meta_pass_id: new_tail_for_meta_pass,

                            maybe_global_index_id: new_tail_for_gi,
                            maybe_mem_pool_id: new_tail_for_mem_pool.clone(),
                        };

                        self
                            .save_updated_db_tail(&logger, client_repo_rc.clone(), db_tail, new_db_tail.clone())
                            .await;

                        let sync_request = {
                            let vault_id_request = match &new_db_tail.vault_id {
                                DbTailObject::Empty { unit_id } => unit_id.clone(),
                                DbTailObject::Id { tail_id } => tail_id.next()
                            };

                            let meta_pass_id_request = match &new_db_tail.meta_pass_id {
                                DbTailObject::Empty { unit_id } => unit_id.clone(),
                                DbTailObject::Id { tail_id } => tail_id.next()
                            };

                            SyncRequest {
                                sender_pk: PublicKeyRecord {
                                    pk: client_creds.user_sig.public_key.as_ref().clone()
                                },
                                global_index: new_db_tail.maybe_global_index_id.clone().map(|gi| gi.next()),
                                vault_tail_id: Some(vault_id_request),
                                meta_pass_tail_id: Some(meta_pass_id_request),
                            }
                        };

                        let mut latest_gi = new_db_tail.maybe_global_index_id.clone();
                        let mut latest_vault_id = new_db_tail.vault_id.clone();
                        let mut latest_meta_pass_id = new_db_tail.meta_pass_id.clone();

                        let server_data_sync = get_data_sync(server_repo_rc, &server_creds);
                        let new_events_res = server_data_sync
                            .sync_data(sync_request)
                            .await;

                        match new_events_res {
                            Ok(new_events) => {
                                for new_event in new_events {
                                    let save_op = client_repo_rc
                                        .save_event(&new_event)
                                        .await;

                                    match save_op {
                                        Ok(()) => {
                                            match new_event {
                                                GenericKvLogEvent::GlobalIndex(_) => {
                                                    latest_gi = Some(new_event.key().obj_id.clone())
                                                }
                                                GenericKvLogEvent::Vault(_) => {
                                                    latest_vault_id = DbTailObject::Id {
                                                        tail_id: new_event.key().obj_id.clone()
                                                    }
                                                }
                                                GenericKvLogEvent::MetaPass(_) => {
                                                    latest_meta_pass_id = DbTailObject::Id {
                                                        tail_id: new_event.key().obj_id.clone()
                                                    }
                                                }
                                                _ => {
                                                    //ignore any non global event
                                                }
                                            }
                                        }
                                        Err(_) => {
                                            logger.log("Error saving new events to local db");
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

                                self
                                    .save_updated_db_tail(&logger, client_repo_rc, new_db_tail.clone(), latest_db_tail)
                                    .await
                            }
                            Err(_err) => {
                                logger.log("DataSync error. Error loading events");
                                panic!("Error");
                            }
                        }
                    }
                    Err(_) => {
                        log("Error! Db tail not exists");
                        panic!("Error");
                    }
                }
            }
        }
    }

    async fn save_updated_db_tail(
        &self, logger: &WasmMetaLogger, client_repo_rc: Rc<WasmRepo>, db_tail: DbTail, new_db_tail: DbTail) {
        if new_db_tail == db_tail {
            return;
        }

        //update db_tail
        let new_db_tail_event = GenericKvLogEvent::LocalEvent(KvLogEventLocal::DbTail {
            event: Box::new(KvLogEvent {
                key: KvKey::unit(&ObjectDescriptor::DbTail),
                value: new_db_tail.clone(),
            })
        });

        let saved_event_res = client_repo_rc
            .save_event(&new_db_tail_event)
            .await;

        match saved_event_res {
            Ok(()) => {
                logger.log("New db tail saved on client side")
            }
            Err(_) => {
                logger.log("Error saving db tail");
            }
        };
    }

    async fn get_new_tail_for_an_obj(
        &self,
        logger: &WasmMetaLogger, server_repo_rc: &Rc<WasmRepo>,
        client_persistent_object: &PersistentObject<WasmRepo, WasmDbError>, server_creds: &UserCredentials,
        db_tail_obj: &DbTailObject,
    ) -> DbTailObject {
        match db_tail_obj {
            DbTailObject::Empty { unit_id } => {
                let maybe_tail_id = client_persistent_object.find_tail_id(&unit_id).await;
                match maybe_tail_id {
                    None => {
                        DbTailObject::Empty { unit_id: unit_id.clone() }
                    }
                    Some(tail_id) => {
                        DbTailObject::Id { tail_id }
                    }
                }
            }
            DbTailObject::Id { tail_id } => {
                let tail_id_sync = match tail_id {
                    ObjectId::Unit { .. } => {
                        tail_id.clone()
                    }
                    _ => {
                        tail_id.next()
                    }
                };

                let obj_events = client_persistent_object
                    .find_object_events(&tail_id_sync)
                    .await;
                let last_vault_event = obj_events.last().cloned();

                let mut client_events: Vec<GenericKvLogEvent> = vec![];
                client_events.extend(obj_events);

                let server_data_sync = get_data_sync(server_repo_rc.clone(), server_creds);
                for client_event in client_events {
                    logger.log(format!("send event to server: {:?}", client_event).as_str());
                    server_data_sync.send_data(&client_event).await;
                }

                let new_tail_id = last_vault_event
                    .map(|event| event.key().obj_id.clone())
                    .unwrap_or(tail_id.clone());
                DbTailObject::Id { tail_id: new_tail_id }
            }
        }
    }

    async fn get_new_tail_for_global_index(
        &self, client_persistent_object: &PersistentObject<WasmRepo, WasmDbError>, db_tail: &DbTail) -> Option<ObjectId> {
        let global_index = db_tail.maybe_global_index_id
            .clone()
            .unwrap_or(ObjectId::global_index_unit());

        client_persistent_object
            .find_tail_id(&global_index)
            .await
    }

    async fn get_new_tail_for_mem_pool(
        &self,
        client_persistent_object: &PersistentObject<WasmRepo, WasmDbError>,
        db_tail: &DbTail, logger: &WasmMetaLogger,
        server_repo_rc: &Rc<WasmRepo>,
        server_creds: &UserCredentials,
    ) -> Option<ObjectId> {
        let mem_pool_id = match db_tail.maybe_mem_pool_id.clone() {
            None => {
                ObjectId::mempool_unit()
            }
            Some(obj_id) => {
                obj_id.next()
            }
        };

        let mem_pool_events = client_persistent_object
            .find_object_events(&mem_pool_id)
            .await;
        let last_pool_event = mem_pool_events.last().cloned();

        let mut client_requests: Vec<GenericKvLogEvent> = vec![];
        client_requests.extend(mem_pool_events);

        let server_data_sync = get_data_sync(server_repo_rc.clone(), server_creds);
        for client_event in client_requests {
            logger.log(format!("send mem pool request to server: {:?}", client_event).as_str());
            server_data_sync.send_data(&client_event).await;
        }

        match last_pool_event {
            None => {
                db_tail.maybe_mem_pool_id.clone()
            }
            Some(event) => {
                Some(event.key().obj_id.clone())
            }
        }
    }

    async fn generate_server_user_credentials(&self, server_repo: Rc<WasmRepo>) {
        log("Generate user credentials for server");

        let logger = WasmMetaLogger {};

        let meta_vault = server_repo
            .create_meta_vault(
                "q".to_string(),
                "meta-server-device".to_string(),
                &logger,
            )
            .await
            .unwrap();

        let security_box = KeyManager::generate_security_box(meta_vault.name);
        let user_sig = security_box.get_user_sig(&meta_vault.device);
        let creds = UserCredentials::new(security_box, user_sig);

        server_repo
            .save_user_creds(creds.clone())
            .await
            .unwrap();

        Self::generate_vault(server_repo, &creds).await;
    }

    async fn generate_vault(server_repo: Rc<WasmRepo>, server_creds: &UserCredentials) {
        let logger = WasmMetaLogger {};

        let sign_up_request_factory = SignUpRequest {};
        let sign_up_request = sign_up_request_factory.generic_request(&server_creds.user_sig);

        let server_data_sync = get_data_sync(server_repo.clone(), server_creds);
        server_data_sync.send_data(&sign_up_request).await;
    }
}

/*
let a_s_box = KeyManager::generate_security_box("qwe".to_string());
    let a_device = DeviceInfo {
        device_id: "a".to_string(),
        device_name: "a".to_string(),
    };
    let user_sig = a_s_box.get_user_sig(&a_device);

    let event = KvLogEvent {
        key: KvKey::formation(&ObjectDescriptor::Tail),
        value: DbTail {
            vault: ObjectId::formation(&ObjectDescriptor::Vault { name: "test_vault".to_string() }),
            global_index: ObjectId::formation(&ObjectDescriptor::GlobalIndex),
        },
    };
    let generic_evt = GenericKvLogEvent::Local(KvLogEventLocal::Tail { event });

    alert("yay!!!");

    meta_vault_manager.save_event(&generic_evt).await;
    meta_vault_manager.save_event(&generic_evt).await;
*/