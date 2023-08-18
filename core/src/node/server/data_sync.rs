use std::error::Error;
use std::rc::Rc;

use async_trait::async_trait;

use crate::crypto::key_pair::KeyPair;
use crate::crypto::keys::KeyManager;
use crate::models::{UserCredentials, UserSignature};
use crate::node::db::actions::join;
use crate::node::db::actions::sign_up::SignUpAction;
use crate::node::db::events::common::ObjectCreator;
use crate::node::db::events::common::{MempoolObject, MetaPassObject, PublicKeyRecord};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, IdStr, ObjectId};
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_manager::MetaDbManager;
use crate::node::db::meta_db::meta_db_view::{MetaDb, VaultStore};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::request::SyncRequest;

pub trait MetaLogger {
    fn log(&self, msg: &str);
    fn id(&self) -> LoggerId;
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoggerId {
    Client,
    Server,
    Vd1,
    Vd2,
}

pub struct DefaultMetaLogger {
    pub id: LoggerId,
}

impl MetaLogger for DefaultMetaLogger {
    fn log(&self, msg: &str) {
        println!("{:?}", msg);
    }

    fn id(&self) -> LoggerId {
        self.id.clone()
    }
}

impl DefaultMetaLogger {
    pub fn new(id: LoggerId) -> Option<Self> {
        Some(Self { id })
    }
}

#[async_trait(? Send)]
pub trait DataSyncApi {
    async fn replication(&self, request: SyncRequest) -> Result<Vec<GenericKvLogEvent>, Box<dyn Error>>;
    async fn send(&self, event: &GenericKvLogEvent);
}

pub struct DataSync {
    pub persistent_obj: Rc<PersistentObject>,
    pub repo: Rc<dyn KvLogEventRepo>,
    pub context: Rc<MetaServerContextState>,
    pub meta_db_manager: Rc<MetaDbManager>,
    pub logger: Rc<dyn MetaLogger>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncMessage {
    SyncRequest(SyncRequest),
    Event(GenericKvLogEvent),
}

//MetaServerContext
#[async_trait(? Send)]
impl DataSyncApi for DataSync {
    async fn replication(&self, request: SyncRequest) -> Result<Vec<GenericKvLogEvent>, Box<dyn Error>> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

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
                let mut meta_db = {
                    let mut meta_db = MetaDb::new(String::from("server"), self.logger.clone());
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

                let vault_signatures = match &meta_db.vault_store {
                    VaultStore::Empty => {
                        self.logger.log("Empty vault store");
                        vec![]
                    }
                    VaultStore::Unit { tail_id } => self.get_user_sig(tail_id).await,
                    VaultStore::Genesis { tail_id, .. } => self.get_user_sig(tail_id).await,
                    VaultStore::Store { vault, .. } => vault.signatures.clone(),
                };

                let vault_signatures: Vec<String> = vault_signatures
                    .iter()
                    .map(|sig| sig.public_key.base64_text.clone())
                    .collect();

                if vault_signatures.contains(&request.sender_pk.pk.base64_text) {
                    let vault_events = self.persistent_obj.find_object_events(vault_tail_id).await;
                    commit_log.extend(vault_events);
                } else {
                    self.logger.log(
                        format!(
                            "The client is not a member of the vault. Client pk: {:?}, vault: {:?}",
                            &request.sender_pk, meta_db.vault_store
                        )
                        .as_str(),
                    );
                    self.logger.log(
                        format!(
                            "Vault sigs: {:?}, sender sig: {:?}",
                            vault_signatures, &request.sender_pk.pk.base64_text
                        )
                        .as_str(),
                    );
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

        for event in &commit_log {
            self.logger
                .log(format!("Serer. Replication. New events: {:?}", event).as_str());
        }
        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled and the actions will be executed accordingly
    async fn send(&self, generic_event: &GenericKvLogEvent) {
        self.server_processing(generic_event).await;
    }
}

impl DataSync {
    async fn server_processing(&self, generic_event: &GenericKvLogEvent) {
        self.logger
            .log(format!("DataSync::event processing: {:?}", generic_event).as_str());

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
                        let _ = self.repo.save_event(generic_event).await;
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

            GenericKvLogEvent::SecretShare(_) => {
                let _ = self.repo.save_event(generic_event).await;
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

impl DataSync {
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

    async fn get_vault_unit_signature(&self, tail_id: &ObjectId) -> Result<Option<UserSignature>, Box<dyn Error>> {
        let maybe_unit_event = self.repo.find_one(tail_id).await?;

        match maybe_unit_event {
            Some(GenericKvLogEvent::Vault(VaultObject::Unit { event })) => Ok(Some(event.value)),
            _ => Ok(None),
        }
    }
}

impl DataSync {
    async fn accept_sign_up_request(&self, event: &KvLogEvent<UserSignature>, vault_id: &IdStr) {
        //vault not found, we can create our new vault
        let server_pk = self.context.server_pk();
        let sign_up_action = SignUpAction {};
        let sign_up_events = sign_up_action.accept(event, &server_pk);

        self.logger.log("ACCEPT SIGN UP REQUEST!!!!!!!!!!!!!11");

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

#[cfg(test)]
mod test {
    use super::*;
    use crate::models::DeviceInfo;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use std::rc::Rc;

    #[tokio::test]
    async fn test_accept_sign_up() {}

    #[tokio::test]
    async fn test() {
        let repo = Rc::new(InMemKvLogEventRepo::default());
        let logger = Rc::new(DefaultMetaLogger { id: LoggerId::Client });

        let persistent_object = Rc::new(PersistentObject::new(repo.clone(), logger.clone()));
        let meta_db_manager = Rc::new(MetaDbManager::from(persistent_object.clone()));

        let s_box = KeyManager::generate_security_box("test_vault".to_string());
        let device = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let user_sig = s_box.get_user_sig(&device);
        let user_creds = UserCredentials {
            security_box: Box::new(s_box),
            user_sig: Box::new(user_sig.clone()),
        };

        let data_sync = DataSync {
            persistent_obj: persistent_object,
            repo,
            context: Rc::new(MetaServerContextState::from(&user_creds)),
            meta_db_manager,
            logger,
        };

        let vault_unit = GenericKvLogEvent::Vault(VaultObject::unit(&user_sig));
        data_sync.send(&vault_unit).await;

        let user_pk = PublicKeyRecord::from(user_sig.public_key.as_ref().clone());

        let request = SyncRequest {
            sender_pk: user_pk,
            vault_tail_id: Some(ObjectId::vault_unit("test_vault")),
            meta_pass_tail_id: None,
            global_index: None,
        };
        let events = data_sync.replication(request).await.unwrap();

        let db_events = data_sync
            .persistent_obj
            .find_tail_id_by_obj_desc(&ObjectDescriptor::vault(String::from("test_vault")))
            .await;

        //println!("{:?}", events.iter().len());
        println!("{:?}", events.len());
    }
}
