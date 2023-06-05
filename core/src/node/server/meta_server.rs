use crate::crypto::utils::to_id;
use crate::node::db::models::{KvLogEvent, MetaDb, VaultStore};
use async_trait::async_trait;
use std::rc::Rc;

use crate::crypto::key_pair::KeyPair;
use crate::crypto::keys::KeyManager;
use crate::models::Base64EncodedText;
use crate::node::db::commit_log::transform;
use crate::node::db::events::global_index;
use crate::node::db::events::global_index::{
    generate_global_index_formation_event, new_global_index_record_created_event,
};
use crate::node::db::events::join::accept_join_request;
use crate::node::db::events::sign_up::accept_event_sign_up_request;
use crate::node::db::meta_db::CommitLogStore;
use crate::node::db::models::{
    AppOperation, AppOperationType, KeyIdGen, KvKeyId,
};

use serde::{Serialize, Deserialize};

#[derive(Deserialize, Serialize)]
pub struct SyncRequest {
    pub vault: Option<VaultSyncRequest>,
    pub global_index: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct VaultSyncRequest {
    pub vault_id: Option<String>,
    pub tail_id: Option<String>,
}

impl From<&MetaDb> for SyncRequest {
    fn from(meta_db: &MetaDb) -> Self {
        let global_index = meta_db
            .global_index_store
            .tail_id.clone()
            .map(|tail_id| tail_id.key_id);

        Self {
            vault: Some(VaultSyncRequest::from(&meta_db.vault_store)),
            global_index,
        }
    }
}

impl From<&VaultStore> for VaultSyncRequest {
    fn from(vault_store: &VaultStore) -> Self {
        let vault_id = vault_store
            .vault.clone()
            .map(|vault| to_id(vault.vault_name.as_str()));

        Self {
            vault_id,
            tail_id: vault_store.tail_id.clone().map(|tail_id| tail_id.key_id),
        }
    }
}

#[async_trait(? Send)]
pub trait MetaServer {
    async fn sync(&self, request: SyncRequest) -> Vec<KvLogEvent>;
    async fn send(&self, event: &KvLogEvent);
}

pub struct MetaServerNode<T: CommitLogStore> {
    pub km: KeyManager,
    pub global_index_tail_id: Option<KvKeyId>,
    pub store: T,
}

impl<T: CommitLogStore> MetaServerNode<T> {
    /// conn_url="file:///tmp/test.db"
    pub fn new(log_store: T) -> Self {
        let km = KeyManager::generate();
        let global_index_tail_id = None;
        Self { km, global_index_tail_id, store: log_store }
    }

    pub fn server_pk(&self) -> Base64EncodedText {
        self.km.dsa.public_key()
    }
}

impl<T: CommitLogStore> MetaServerNode<T> {

    async fn get_next_free_global_index_id(&self) -> KvKeyId {
        let formation_id = global_index::generate_global_index_formation_key_id();

        let mut existing_id = formation_id.clone();
        let mut curr_tail_id = formation_id;
        loop {
            let global_idx_result = self.store.find_one(curr_tail_id.key_id.as_str()).await;

            match global_idx_result {
                Ok(maybe_idx) => {
                    match maybe_idx {
                        Some(_idx) => {
                            existing_id = curr_tail_id.clone();
                            curr_tail_id = curr_tail_id.next();
                        }
                        None => {
                            break;
                        }
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }

        existing_id
    }

    async fn get_global_index_from_beginning(&self) -> Vec<KvLogEvent> {
        let formation_id = global_index::generate_global_index_formation_key_id();
        let mut commit_log = self.find_object_events(formation_id.key_id.as_str()).await;

        //check if genesis event exists for vaults index
        if commit_log.is_empty() {
            //create a genesis event and save into the database
            let formation_event =
                generate_global_index_formation_event(&self.km.dsa.public_key());

            self.store.save("", &formation_event)
                .await
                .expect("Error saving vaults genesis event");

            commit_log.push(formation_event);
        }

        commit_log
    }

    async fn find_object_events(&self, tail_id: &str) -> Vec<KvLogEvent> {
        let mut commit_log = vec![];

        let mut curr_tail_id = tail_id.to_string();
        loop {
            let curr_db_event_result = self.store.find_one(curr_tail_id.as_str()).await;

            match curr_db_event_result {
                Ok(maybe_curr_db_event) => {
                    match maybe_curr_db_event {
                        Some(curr_db_event) => {
                            commit_log.push(curr_db_event);
                            curr_tail_id = KvKeyId::generate_next(curr_tail_id.as_str()).key_id.clone();
                        }
                        None => {
                            break;
                        }
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }

        commit_log
    }
}

#[async_trait(? Send)]
impl<T: CommitLogStore> MetaServer for MetaServerNode<T> {
    async fn sync(&self, request: SyncRequest) -> Vec<KvLogEvent> {
        let mut commit_log = vec![];

        match request.global_index {
            None => {
                let meta_g = self.get_global_index_from_beginning().await;
                commit_log.extend(meta_g);
            }
            Some(index_id) => {
                let meta_g = self.find_object_events(index_id.as_str()).await;
                commit_log.extend(meta_g);
            }
        }

        match request.vault {
            None => {
                // Ignore empty requests
            }
            Some(vault_request) => {
                let vault_and_tail = (vault_request.vault_id, vault_request.tail_id);

                match vault_and_tail {
                    (Some(request_vault_id), None) => {
                        //get all types of objects and build a commit log

                        let vault_events = self
                            .find_object_events(request_vault_id.as_str())
                            .await;

                        println!("sync. events num: {:?}", vault_events.len());

                        commit_log.extend(vault_events);
                    }
                    _ => {
                        println!("no need to do any actions");
                    }
                }
            }
        }

        commit_log
    }

    /// Handle request: all types of requests will be handled and the actions will be executed accordingly
    async fn send(&self, event: &KvLogEvent) {
        match event.cmd_type {
            AppOperationType::Request(op) => {
                match op {
                    AppOperation::ObjectFormation => {
                        panic!("Not allowed");
                    }

                    AppOperation::GlobalIndex => {
                        panic!("Not allowed");
                    }

                    AppOperation::SignUp => {
                        // Handled by the server. Add a vault to the system
                        let vault_id = event.key.vault_id.clone().unwrap();
                        let vault_formation_id = KvKeyId::object_foundation_from_id(vault_id.as_str());

                        let vault_formation_event_result = self.store
                            .find_one(vault_formation_id.key_id.as_str())
                            .await;

                        match vault_formation_event_result {
                            Err(_) => {
                                self.accept_sign_up_request(event, vault_id).await;
                            }
                            Ok(_sign_up_event) => {
                                panic!("Vault already exists");
                                //save a reject response in the database
                            }
                        }
                    }

                    AppOperation::JoinCluster => {
                        let vault_id = event.key.vault_id.clone();
                        match vault_id {
                            None => {
                                panic!("Invalid JoinCluster request: vault is not set");
                            }
                            Some(vault_id) => {
                                self.accept_join_cluster_request(event, vault_id).await;
                            }
                        }
                    }
                }
            }

            //Check validity and just save to the database
            AppOperationType::Update(op) => match op {
                AppOperation::JoinCluster => {}
                AppOperation::GlobalIndex => {}

                AppOperation::ObjectFormation => {
                    //Skip an update operation (an update generated by server)
                }
                AppOperation::SignUp => {
                    //Skip an update operation (an update generated by server)
                }
            },
        }
    }
}

impl<T: CommitLogStore> MetaServerNode<T> {
    async fn accept_join_cluster_request(&self, join_event: &KvLogEvent, vault_id: String) {
        println!("save join request: {}", serde_json::to_string(&join_event).unwrap());

        self.store.save("", join_event)
            .await
            .expect("Error saving join request");

        //join cluster update message
        let vault_events = self
            .find_object_events(vault_id.as_str())
            .await;

        let meta_db = transform(Rc::new(vault_events));

        let vault_doc = &meta_db.unwrap().vault_store.vault.unwrap();
        let accept_event = accept_join_request(join_event, vault_doc);

        self.store.save("", &accept_event)
            .await
            .expect("Error saving accept event");
    }
}

impl<T: CommitLogStore> MetaServerNode<T> {
    async fn accept_sign_up_request(&self, event: &KvLogEvent, vault_id: String) {
//vault not found, we can create our new vault
        let sign_up_events = accept_event_sign_up_request(
            event.clone(),
            self.server_pk(),
        );

        //find the latest global_index_id???
        let global_index_tail_id =
            match self.global_index_tail_id.clone() {
                None => {
                    self.get_next_free_global_index_id().await
                }
                Some(tail_id) => {
                    //we already have latest global index id
                    tail_id
                }
            };

        for sign_up_event in sign_up_events {
            self.store.save("", &sign_up_event)
                .await
                .expect("Error saving sign_up events");
        }

        //update global index
        let global_index_event = new_global_index_record_created_event(
            &global_index_tail_id,
            vault_id.as_str(),
        );

        self.store.save("", &global_index_event)
            .await
            .expect("Error saving vaults genesis event");
    }
}
