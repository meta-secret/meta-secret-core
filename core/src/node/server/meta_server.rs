use std::error::Error;

use async_trait::async_trait;

use crate::crypto::key_pair::KeyPair;
use crate::crypto::keys::KeyManager;
use crate::models::UserSignature;
use crate::node::db::commit_log::MetaDbManager;
use crate::node::db::events::join;
use crate::node::db::events::sign_up::SignUpAction;
use crate::node::db::generic_db::{KvLogEventRepo};
use crate::node::db::models::{Descriptors, KvLogEvent, ObjectCreator, ObjectDescriptor};
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::models::{GenericKvLogEvent, KvKeyId, KvLogEventRequest, KvLogEventUpdate, PublicKeyRecord};
use crate::node::server::persistent_object_repo::PersistentObject;
use crate::node::server::request::SyncRequest;
use std::rc::Rc;

#[async_trait(? Send)]
pub trait DataSyncApi<Err> {
    async fn sync_data(&self, request: SyncRequest) -> Result<Vec<GenericKvLogEvent>, Err>;
    async fn send_data(&self, event: &GenericKvLogEvent);
}

pub struct DataSync<Repo: KvLogEventRepo<Err>, Err: Error> {
    pub persistent_obj: Rc<PersistentObject<Repo, Err>>,
    pub repo: Rc<Repo>,
    pub context: Rc<MetaServerContextState>,
    pub meta_db_manager: MetaDbManager<Repo, Err>,
}

//MetaServerContext
#[async_trait(? Send)]
impl<Repo: KvLogEventRepo<Err>, Err: Error> DataSyncApi<Err> for DataSync<Repo, Err> {

    async fn sync_data(&self, request: SyncRequest) -> Result<Vec<GenericKvLogEvent>, Err> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        match request.global_index {
            None => {
                let descriptor = Descriptors::global_index();
                let meta_g = self
                    .persistent_obj
                    .get_object_events_from_beginning(&descriptor, &self.context.server_pk())
                    .await?;
                commit_log.extend(meta_g);
            }
            Some(index_id) => {
                let meta_g = self
                    .persistent_obj
                    .find_object_events(&index_id)
                    .await;

                commit_log.extend(meta_g);
            }
        }

        match request.vault {
            None => {
                // Ignore empty requests
            }
            Some(vault_request) => {
                match vault_request.tail_id {
                    Some(request_tail_id) => {
                        //get all types of objects and build a commit log

                        let vault_events = self
                            .persistent_obj
                            .find_object_events(&request_tail_id)
                            .await;

                        println!("sync. events num: {:?}", vault_events.len());

                        commit_log.extend(vault_events);
                    }
                    None => {
                        println!("no need to do any actions");
                    }
                }
            }
        }

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled and the actions will be executed accordingly
    async fn send_data(&self, generic_event: &GenericKvLogEvent) {
        match generic_event {
            GenericKvLogEvent::Request(request) => {
                match request {
                    KvLogEventRequest::SignUp { event } => {
                        // Handled by the server. Add a vault to the system
                        let vault_id = event.key.key_id.obj_id().genesis_id();
                        let vault_formation_event_result = self
                            .repo
                            .find_one(&vault_id)
                            .await;

                        match vault_formation_event_result {
                            Err(_) => {
                                self.accept_sign_up_request(event, &vault_id).await;
                            }
                            Ok(_sign_up_event) => {
                                panic!("Vault already exists");
                                //save a reject response in the database
                            }
                        }
                    }
                    KvLogEventRequest::JoinCluster { event } => {
                        let user_sig: UserSignature = event.value.clone();
                        let obj_desc = ObjectDescriptor::Vault { name: user_sig.vault.name };
                        let vault_id = ObjectId::formation(&obj_desc);
                        self.accept_join_cluster_request(event, &vault_id).await;
                    }
                }
            }

            GenericKvLogEvent::Update(op) => match op {
                KvLogEventUpdate::Genesis { .. } => {}
                KvLogEventUpdate::GlobalIndex { .. } => {
                    panic!("Not allowed");
                }
                KvLogEventUpdate::SignUp { .. } => {
                    panic!("Not allowed");
                }
                KvLogEventUpdate::JoinCluster { .. } => {
                    panic!("Not allowed");
                }
            },
            GenericKvLogEvent::MetaVault { .. } => {
                panic!("Not allowed");
            }
            GenericKvLogEvent::UserCredentials { .. } => {
                panic!("Not allowed");
            }
            GenericKvLogEvent::Error { .. } => {
                panic!("Not allowed");
            }
        }
    }
}

impl<Repo: KvLogEventRepo<Err>, Err: Error> DataSync<Repo, Err> {
    async fn accept_join_cluster_request(&self, join_event: &KvLogEvent<UserSignature>, obj_id: &ObjectId) {
        println!("save join request: {}", serde_json::to_string(&join_event).unwrap());

        let generic_join_event = GenericKvLogEvent::Request(KvLogEventRequest::JoinCluster {
            event: join_event.clone(),
        });
        self
            .repo
            .save_event(&generic_join_event)
            .await
            .expect("Error saving join request");

        //join cluster update message
        let vault_events = self
            .persistent_obj
            .find_object_events(&obj_id.genesis_id())
            .await;

        let meta_db = self.meta_db_manager.transform(vault_events);

        let vault_doc = &meta_db.unwrap().vault_store.vault.unwrap();
        let accept_event = join::accept_join_request(join_event, vault_doc);
        let generic_accept_event = GenericKvLogEvent::Update(KvLogEventUpdate::SignUp { event: accept_event });

        self.
            repo
            .save_event(&generic_accept_event)
            .await
            .expect("Error saving accept event");
    }

    async fn accept_sign_up_request(&self, event: &KvLogEvent<UserSignature>, vault_id: &ObjectId) {
        //vault not found, we can create our new vault
        let server_pk = self.context.server_pk();
        let sign_up_action = SignUpAction {};
        let sign_up_events = sign_up_action.accept(event, &server_pk);

        //find the latest global_index_id???
        let global_index_tail_id = match self.context.tail_id().clone() {
            None => self.persistent_obj.acuire_next_free_id(&Descriptors::global_index()).await,
            Some(tail_id) => {
                //we already have latest global index id
                tail_id
            }
        };

        for sign_up_event in sign_up_events {
            self
                .repo
                .save_event(&sign_up_event)
                .await
                .expect("Error saving sign_up events");
        }

        //update global index
        let global_index_event = KvLogEvent::new_global_index_event(&global_index_tail_id, vault_id);
        let global_index_event = GenericKvLogEvent::Update(KvLogEventUpdate::GlobalIndex {
            event: global_index_event,
        });

        self
            .repo
            .save_event(&global_index_event)
            .await
            .expect("Error saving vaults genesis event");
    }
}

pub trait MetaServerContext {
    fn server_pk(&self) -> PublicKeyRecord;
    fn tail_id(&self) -> Option<KvKeyId>;
}

pub struct MetaServerContextState {
    pub km: KeyManager,
    pub global_index_tail_id: Option<KvKeyId>,
}

impl MetaServerContext for MetaServerContextState {
    fn server_pk(&self) -> PublicKeyRecord {
        PublicKeyRecord::from(self.km.dsa.public_key())
    }

    fn tail_id(&self) -> Option<KvKeyId> {
        self.global_index_tail_id.clone()
    }
}

impl Default for MetaServerContextState {
    /// conn_url="file:///tmp/test.db"
    fn default() -> Self {
        let km = KeyManager::generate();
        let global_index_tail_id = None;
        Self {
            km,
            global_index_tail_id,
        }
    }
}
