use std::error::Error;
use std::rc::Rc;

use async_trait::async_trait;

use crate::crypto::key_pair::KeyPair;
use crate::crypto::keys::KeyManager;
use crate::models::{UserCredentials, UserSignature};
use crate::node::db::commit_log::MetaDbManager;
use crate::node::db::events::join;
use crate::node::db::events::object_id::{IdGen, IdStr, ObjectId};
use crate::node::db::events::sign_up::SignUpAction;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::{MetaDb, VaultStore};
use crate::node::db::models::{GenericKvLogEvent, MempoolObject, MetaPassObject, PublicKeyRecord};
use crate::node::db::models::{GlobalIndexObject, KvLogEvent, ObjectCreator, ObjectDescriptor, VaultObject};
use crate::node::db::persistent_object::PersistentObject;
use crate::node::server::request::SyncRequest;

pub trait MetaLogger {
    fn log(&self, msg: &str);
}

pub struct DefaultMetaLogger {}

impl MetaLogger for DefaultMetaLogger {
    fn log(&self, msg: &str) {
        println!("{:?}", msg);
    }
}

impl DefaultMetaLogger {
    pub fn new() -> Option<Self> {
        Some(Self {})
    }
}

#[async_trait(? Send)]
pub trait DataSyncApi<Err> {
    async fn replication(&self, request: SyncRequest) -> Result<Vec<GenericKvLogEvent>, Err>;
    async fn send(&self, event: &GenericKvLogEvent);
}

pub struct DataSync<Repo: KvLogEventRepo<Err>, L: MetaLogger, Err: Error> {
    pub persistent_obj: Rc<PersistentObject<Repo, L, Err>>,
    pub repo: Rc<Repo>,
    pub context: Rc<MetaServerContextState>,
    pub meta_db_manager: Rc<MetaDbManager<Repo, L, Err>>,
    pub logger: Rc<L>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncMessage {
    SyncRequest(SyncRequest),
    Event(GenericKvLogEvent),
}

//MetaServerContext
#[async_trait(? Send)]
impl<Repo, L, Err> DataSyncApi<Err> for DataSync<Repo, L, Err>
where
    Repo: KvLogEventRepo<Err>,
    L: MetaLogger,
    Err: Error,
{
    async fn replication(&self, request: SyncRequest) -> Result<Vec<GenericKvLogEvent>, Err> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        let mut meta_db = {
            let mut meta_db = MetaDb::default();
            match &request.vault_tail_id {
                None => meta_db.vault_store = VaultStore::Empty,
                Some(request_vault_tail_id) => {
                    meta_db.vault_store = VaultStore::Unit {
                        tail_id: request_vault_tail_id.unit_id(),
                    }
                }
            }

            meta_db
        };

        self.meta_db_manager.sync_meta_db(&mut meta_db).await;

        match &request.global_index {
            None => {
                let meta_g = self
                    .persistent_obj
                    .get_object_events_from_beginning(&ObjectDescriptor::GlobalIndex, &self.context.server_pk())
                    .await?;
                commit_log.extend(meta_g);
            }
            Some(index_id) => {
                let meta_g = self.persistent_obj.find_object_events(index_id).await;
                commit_log.extend(meta_g);
            }
        }

        match &request.vault_tail_id {
            None => {
                // Ignore empty requests
            }
            Some(vault_tail_id) => {
                let vault_signatures = match meta_db.vault_store {
                    VaultStore::Empty => {
                        vec![]
                    }
                    VaultStore::Unit { tail_id } => self.get_user_sig(&tail_id).await,
                    VaultStore::Genesis { tail_id, .. } => self.get_user_sig(&tail_id).await,
                    VaultStore::Store { vault, .. } => vault.signatures,
                };

                let vault_signatures: Vec<PublicKeyRecord> = vault_signatures
                    .iter()
                    .map(|sig| PublicKeyRecord::from(sig.public_key.as_ref().clone()))
                    .collect();

                if vault_signatures.contains(&request.sender_pk) {
                    let vault_events = self.persistent_obj.find_object_events(vault_tail_id).await;
                    commit_log.extend(vault_events);
                }
            }
        }

        match &request.meta_pass_tail_id {
            None => {
                // Ignore empty requests
            }
            Some(meta_pass_tail_id) => {
                let meta_pass_events = self.persistent_obj.find_object_events(meta_pass_tail_id).await;
                commit_log.extend(meta_pass_events);
            }
        }

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled and the actions will be executed accordingly
    async fn send(&self, generic_event: &GenericKvLogEvent) {
        self.server_processing(generic_event).await;
    }
}

impl<Repo: KvLogEventRepo<Err>, L: MetaLogger, Err: Error> DataSync<Repo, L, Err> {
    async fn server_processing(&self, generic_event: &GenericKvLogEvent) {
        self.logger.log("DataSync::event processing");

        match generic_event {
            GenericKvLogEvent::GlobalIndex(_) => {
                self.logger.log("Global index not allowed to be sent");
            }

            GenericKvLogEvent::Vault(vault_obj_info) => {
                match vault_obj_info {
                    VaultObject::Unit { event } => {
                        self.logger.log("Handle 'vault_object:unit' event");
                        // Handled by the server. Add a vault to the system
                        let vault_id = event.key.obj_id.unit_id();

                        self.logger
                            .log(format!("Looking for a vault: {}", vault_id.id_str()).as_str());

                        let vault_formation_event_result = self.repo.find_one(&vault_id).await;

                        let vault_id_str = IdStr::from(&vault_id);

                        match vault_formation_event_result {
                            Err(_) => {
                                self.accept_sign_up_request(event, &vault_id_str).await;
                            }
                            Ok(maybe_sign_up) => match maybe_sign_up {
                                None => {
                                    self.accept_sign_up_request(event, &vault_id_str).await;
                                }
                                Some(_sign_up) => {
                                    self.logger.log("Error. Vault already exists. Skip");
                                }
                            },
                        }
                    }
                    VaultObject::Genesis { .. } => {
                        self.logger.log("Genesis event not allowed to send. Skip");
                    }
                    VaultObject::SignUpUpdate { .. } => {
                        self.logger.log("SignUp update not allowed to send. Skip");
                    }
                    VaultObject::JoinUpdate { .. } => {
                        self.logger.log("Join with update not allowed to send. Skip");
                    }
                    VaultObject::JoinRequest { .. } => {
                        //self.logger.log("Handle join request");
                        //self.accept_join_cluster_request(event).await;
                        self.logger.log("Ignore Join request on server side");
                    }
                }
            }
            GenericKvLogEvent::MetaPass(meta_pass_obj) => match meta_pass_obj {
                MetaPassObject::Unit { .. } => {
                    self.logger.log("Ignore unit event for meta pass");
                }
                MetaPassObject::Genesis { .. } => {
                    self.logger.log("Ignore genesis event for meta pass");
                }
                MetaPassObject::Update { event } => {
                    let meta_pass_event = GenericKvLogEvent::MetaPass(MetaPassObject::Update { event: event.clone() });
                    let save_command = self.repo.save_event(&meta_pass_event).await;

                    if save_command.is_err() {
                        let err_msg = String::from("Error saving meta pass request");
                        self.logger.log(err_msg.as_str());
                        panic!("Error");
                    }
                }
            },
            GenericKvLogEvent::Mempool(evt_type) => {
                // save mempool event in the database
                self.logger.log("Data Sync. Handle mem pool request");
                match evt_type {
                    MempoolObject::JoinRequest { event } => {
                        let vault_name = event.value.vault.name.clone();
                        let vault_obj_id = ObjectId::vault_unit(vault_name.as_str());

                        let maybe_vault_tail_id = self.persistent_obj.find_tail_id(&vault_obj_id).await;

                        match maybe_vault_tail_id {
                            None => {
                                //ignore, vault not exists yet, no way to join vault
                            }
                            Some(vault_tail_id) => {
                                let join_request = GenericKvLogEvent::Vault(VaultObject::JoinRequest {
                                    event: join::join_cluster_request(&vault_tail_id, &event.value),
                                });

                                let _ = self.repo.save_event(&join_request).await;
                            }
                        }
                    }
                }
            }
            GenericKvLogEvent::LocalEvent(evt_type) => {
                self.logger
                    .log(format!("Local events can't be sent: {:?}", evt_type).as_str());
            }
            GenericKvLogEvent::Error { .. } => {
                self.logger.log("Errors not yet implemented");
            }
        }
    }
}

impl<Repo: KvLogEventRepo<Err>, L: MetaLogger, Err: Error> DataSync<Repo, L, Err> {
    async fn get_user_sig(&self, tail_id: &ObjectId) -> Vec<UserSignature> {
        let sig_result = self.get_vault_unit_signature(tail_id).await;
        match sig_result {
            Ok(Some(vault_sig)) => {
                vec![vault_sig]
            }
            _ => {
                vec![]
            }
        }
    }
}

impl<Repo: KvLogEventRepo<Err>, L: MetaLogger, Err: Error> DataSync<Repo, L, Err> {
    async fn get_vault_unit_signature(&self, tail_id: &ObjectId) -> Result<Option<UserSignature>, Err> {
        let maybe_unit_event = self.repo.find_one(tail_id).await?;

        match maybe_unit_event {
            None => Ok(None),
            Some(unit_event) => match unit_event {
                GenericKvLogEvent::Vault(vault_obj) => match vault_obj {
                    VaultObject::Unit { event } => Ok(Some(event.value)),
                    _ => Ok(None),
                },
                _ => Ok(None),
            },
        }
    }
}

impl<Repo: KvLogEventRepo<Err>, L: MetaLogger, Err: Error> DataSync<Repo, L, Err> {
    async fn accept_sign_up_request(&self, event: &KvLogEvent<UserSignature>, vault_id: &IdStr) {
        //vault not found, we can create our new vault
        let server_pk = self.context.server_pk();
        let sign_up_action = SignUpAction {};
        let sign_up_events = sign_up_action.accept(event, &server_pk);

        for sign_up_event in sign_up_events {
            self.repo
                .save_event(&sign_up_event)
                .await
                .expect("Error saving sign_up events");
        }

        //update global index
        //find the latest global_index_id???
        let gi_obj_id = ObjectId::unit(&ObjectDescriptor::GlobalIndex);
        let global_index_tail_id = self.persistent_obj.find_tail_id(&gi_obj_id).await.unwrap_or(gi_obj_id);

        let mut gi_events = vec![];
        if let ObjectId::Unit { id: _ } = global_index_tail_id.clone() {
            let unit_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Unit {
                event: KvLogEvent::global_index_unit(),
            });

            let genesis_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Genesis {
                event: KvLogEvent::global_index_genesis(&server_pk),
            });

            gi_events.push(unit_event);
            gi_events.push(genesis_event);
        }

        let gi_obj_id = match global_index_tail_id {
            ObjectId::Unit { .. } => ObjectId::global_index_unit().next().next(),
            ObjectId::Genesis { .. } => ObjectId::global_index_unit().next(),
            ObjectId::Regular { .. } => global_index_tail_id.next(),
        };

        let gi_update_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Update {
            event: KvLogEvent::new_global_index_event(&gi_obj_id, vault_id),
        });

        gi_events.push(gi_update_event);

        for gi_event in gi_events {
            self.repo
                .save_event(&gi_event)
                .await
                .expect("Error saving vaults genesis event");
        }
    }
}

pub trait MetaServerContext {
    fn server_pk(&self) -> PublicKeyRecord;
}

pub struct MetaServerContextState {
    pub km: KeyManager,
}

impl MetaServerContext for MetaServerContextState {
    fn server_pk(&self) -> PublicKeyRecord {
        PublicKeyRecord::from(self.km.dsa.public_key())
    }
}

impl Default for MetaServerContextState {
    /// conn_url="file:///tmp/test.db"
    fn default() -> Self {
        let km = KeyManager::generate();
        Self { km }
    }
}

impl From<&UserCredentials> for MetaServerContextState {
    fn from(creds: &UserCredentials) -> Self {
        Self {
            km: KeyManager::try_from(creds.security_box.key_manager.as_ref()).unwrap(),
        }
    }
}
