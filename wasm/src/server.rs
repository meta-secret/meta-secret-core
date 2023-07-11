use std::marker::PhantomData;
use std::rc::Rc;

use wasm_bindgen::prelude::*;

use meta_secret_core::crypto::keys::KeyManager;
use meta_secret_core::models::UserCredentials;
use meta_secret_core::node::app::meta_app::{MetaVaultManager, UserCredentialsManager};
use meta_secret_core::node::db::events::object_id::{IdGen, ObjectId};
use meta_secret_core::node::db::generic_db::{SaveCommand};
use meta_secret_core::node::db::models::{
    DbTail, GenericKvLogEvent, KvKey, KvLogEvent, KvLogEventLocal, LogEventKeyBasedRecord, ObjectCreator, ObjectDescriptor
};
use meta_secret_core::node::server::data_sync::{DataSyncApi, MetaLogger};
use meta_secret_core::node::server::persistent_object::{PersistentGlobalIndex, PersistentObject};
use meta_secret_core::node::server::request::{SyncRequest, VaultSyncRequest};

use crate::{alert, log, utils};
use crate::commit_log::{WasmMetaLogger, WasmRepo};
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

        match server_creds_result {
            Ok(maybe_server_creds) => {
                match maybe_server_creds {
                    Some(server_creds) => {
                        match client_creds_result {
                            Ok(maybe_client_creds) => {
                                match maybe_client_creds {
                                    None => {
                                        logger.log("User credentials not exists yet. Skip operations on server");
                                    }
                                    Some(client_creds) => {
                                        //log("Wasm::run_server()");

                                        let db_tail: DbTail = client_persistent_object
                                            .get_db_tail()
                                            .await
                                            .unwrap();

                                        //logger.log(format!("curr db_tail: {:?}", db_tail).as_str());

                                        let maybe_new_tail_for_vault = match db_tail.vault.clone() {
                                            None => {
                                                let vault_name = client_creds.user_sig.vault.name.as_str();
                                                let vault_id = ObjectId::vault_unit(vault_name);
                                                client_persistent_object.find_tail_id(&vault_id).await
                                            }
                                            Some(vault_id) => {
                                                let vault_id_sync = match vault_id {
                                                    ObjectId::Unit { .. } => {
                                                        vault_id.clone()
                                                    }
                                                    _ => {
                                                        vault_id.next()
                                                    }
                                                };

                                                let vault_events = client_persistent_object
                                                    .find_object_events(&vault_id_sync, &logger)
                                                    .await;
                                                let last_vault_event = vault_events.last().cloned();

                                                let mut client_events: Vec<GenericKvLogEvent> = vec![];
                                                client_events.extend(vault_events);

                                                let server_data_sync = get_data_sync(server_repo_rc.clone(), &server_creds);
                                                for client_event in client_events {
                                                    logger.log(format!("send event to server: {:?}", client_event).as_str());
                                                    server_data_sync.send_data(&client_event, &logger).await;
                                                }

                                                Some(last_vault_event
                                                    .map(|event| event.key().obj_id.clone())
                                                    .unwrap_or(vault_id)
                                                )
                                            }
                                        };

                                        let new_tail_for_global_index = match db_tail.global_index.clone() {
                                            None => {
                                                client_persistent_object
                                                    .find_tail_id(&ObjectId::global_index_unit())
                                                    .await
                                            }
                                            Some(global_index) => {
                                                let maybe_last_gi_event = client_persistent_object
                                                    .find_tail_id(&global_index)
                                                    .await;

                                                match maybe_last_gi_event {
                                                    None => {
                                                        //alert("Latest Global index not found!");
                                                        Some(global_index)
                                                    }
                                                    Some(updated_gi) => {
                                                        //alert(format!("latest gi: {:?}", updated_gi).as_str());
                                                        Some(updated_gi)
                                                    }
                                                }
                                            }
                                        };

                                        let new_db_tail = {
                                            DbTail {
                                                vault: maybe_new_tail_for_vault,
                                                global_index: new_tail_for_global_index,
                                            }
                                        };

                                        if new_db_tail != db_tail {
                                            //update db_tail
                                            let new_db_tail_event = GenericKvLogEvent::LocalEvent(KvLogEventLocal::Tail {
                                                event: KvLogEvent {
                                                    key: KvKey::unit(&ObjectDescriptor::DbTail),
                                                    value: new_db_tail.clone(),
                                                }
                                            });

                                            client_repo_rc.save_event(&new_db_tail_event)
                                                .await
                                                .unwrap();
                                        }

                                        let request = SyncRequest {
                                            vault: Some(VaultSyncRequest {
                                                tail_id: new_db_tail.vault.clone().map(|vault_id| vault_id.next()),
                                            }),
                                            global_index: new_db_tail.global_index.clone().map(|gi| gi.next()),
                                        };

                                        let server_data_sync = get_data_sync(server_repo_rc, &server_creds);
                                        let new_events = server_data_sync
                                            .sync_data(request, &logger)
                                            .await
                                            .expect("Error syncing data");

                                        let mut latest_gi = new_db_tail.global_index.clone();
                                        let mut latest_vault_id = new_db_tail.vault.clone();

                                        for new_event in new_events {
                                            client_repo_rc
                                                .save_event(&new_event)
                                                .await
                                                .expect("Error syncing data");

                                            match new_event {
                                                GenericKvLogEvent::GlobalIndex(_) => {
                                                    latest_gi = Some(new_event.key().obj_id.clone())
                                                }
                                                GenericKvLogEvent::Vault(_) => {
                                                    latest_vault_id = Some(new_event.key().obj_id.clone())
                                                }
                                                _ => {

                                                }
                                            }
                                        }

                                        let latest_db_tail = {
                                            DbTail {
                                                vault: latest_vault_id,
                                                global_index: latest_gi,
                                            }
                                        };

                                        if latest_db_tail != new_db_tail {
                                            //update db_tail
                                            let latest_db_tail_event = GenericKvLogEvent::LocalEvent(KvLogEventLocal::Tail {
                                                event: KvLogEvent {
                                                    key: KvKey::unit(&ObjectDescriptor::DbTail),
                                                    value: latest_db_tail.clone(),
                                                }
                                            });

                                            client_repo_rc.save_event(&latest_db_tail_event)
                                                .await
                                                .unwrap();
                                        }
                                    }
                                }
                            }
                            Err(_) => {
                                logger.log("User credentials not exists yet. Skip operations on server");
                            }
                        }
                    }
                    None => {
                        Self::generate_server_user_credentials(server_repo_rc).await;
                    }
                }
            }
            Err(_) => {
                Self::generate_server_user_credentials(server_repo_rc).await;
            }
        }
    }

    async fn generate_server_user_credentials(server_repo: Rc<WasmRepo>) {
        log("Generate user credentials for server");

        let logger = WasmMetaLogger {};

        let meta_vault = server_repo
            .create_meta_vault(
                "meta-server-vault".to_string(),
                "meta-server-device".to_string(),
                &logger,
            )
            .await
            .unwrap();

        let security_box = KeyManager::generate_security_box(meta_vault.name);
        let user_sig = security_box.get_user_sig(&meta_vault.device);
        let creds = UserCredentials::new(security_box, user_sig);

        server_repo
            .save_user_creds(creds)
            .await
            .unwrap();
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