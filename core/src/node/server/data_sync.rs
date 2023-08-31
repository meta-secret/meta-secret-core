use std::rc::Rc;

use async_trait::async_trait;

use crate::crypto::key_pair::KeyPair;
use crate::crypto::keys::KeyManager;
use crate::models::{UserCredentials, UserSignature};
use crate::node::db::actions::join;
use crate::node::db::actions::sign_up::SignUpAction;
use crate::node::db::events::common::{LogEventKeyBasedRecord, ObjectCreator, SharedSecretObject};
use crate::node::db::events::common::{MempoolObject, MetaPassObject, PublicKeyRecord};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, IdStr, ObjectId};
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_manager::MetaDbManager;
use crate::node::db::meta_db::meta_db_view::{MetaDb, VaultStore};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::{MetaLogger};
use crate::node::server::request::SyncRequest;

#[async_trait(? Send)]
pub trait DataSyncApi {
    async fn replication(&self, request: SyncRequest) -> anyhow::Result<Vec<GenericKvLogEvent>>;
    async fn send(&self, event: &GenericKvLogEvent);
}

pub struct DataSync<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub persistent_obj: Rc<PersistentObject<Repo, Logger>>,
    pub repo: Rc<Repo>,
    pub context: Rc<MetaServerContextState>,
    pub meta_db_manager: Rc<MetaDbManager<Repo, Logger>>,
    pub logger: Rc<Logger>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncMessage {
    SyncRequest(SyncRequest),
    Event(GenericKvLogEvent),
}

//MetaServerContext
#[async_trait(? Send)]
impl<Repo: KvLogEventRepo, Logger: MetaLogger> DataSyncApi for DataSync<Repo, Logger> {
    async fn replication(&self, request: SyncRequest) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        self.global_index_replication(&request, &mut commit_log).await?;

        self.vault_replication(&request, &mut commit_log).await;

        self.meta_pass_replication(&request, &mut commit_log).await;

        self.shared_secret_replication(&request, &mut commit_log).await;

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled and the actions will be executed accordingly
    async fn send(&self, generic_event: &GenericKvLogEvent) {
        self.server_processing(generic_event).await;
    }
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> DataSync<Repo, Logger> {
    async fn server_processing(&self, generic_event: &GenericKvLogEvent) {
        self.logger
            .debug(format!("DataSync::event_processing: {:?}", generic_event).as_str());

        match generic_event {
            GenericKvLogEvent::GlobalIndex(_) => {
                self.logger.info("Global index not allowed to be sent");
            }

            GenericKvLogEvent::Vault(vault_obj_info) => {
                match vault_obj_info {
                    VaultObject::Unit { event } => {
                        self.logger.info("Handle 'vault_object:unit' event");
                        // Handled by the server. Add a vault to the system
                        let vault_id = match &event.key {
                            KvKey::Empty { .. } => {
                                panic!("Invalid event")
                            }
                            KvKey::Key { obj_id, .. } => {
                                obj_id.unit_id()
                            }
                        };

                        self.logger
                            .info(format!("Looking for a vault: {}", vault_id.id_str()).as_str());

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
                                    self.logger.info("Error. Vault already exists. Skip");
                                }
                            },
                        }
                    }
                    VaultObject::Genesis { .. } => {
                        self.logger.info("Genesis event not allowed to send. Skip");
                    }
                    VaultObject::SignUpUpdate { .. } => {
                        self.logger.info("SignUp update not allowed to send. Skip");
                    }
                    VaultObject::JoinUpdate { .. } => {
                        let _ = self.repo.save_event(generic_event).await;
                    }
                    VaultObject::JoinRequest { .. } => {
                        //self.logger.log("Handle join request");
                        //self.accept_join_cluster_request(event).await;
                        self.logger.info("Ignore Join request on server side");
                    }
                }
            }
            GenericKvLogEvent::MetaPass(meta_pass_obj) => match meta_pass_obj {
                MetaPassObject::Unit { .. } => {
                    self.logger.info("Ignore unit event for meta pass");
                }
                MetaPassObject::Genesis { .. } => {
                    self.logger.info("Ignore genesis event for meta pass");
                }
                MetaPassObject::Update { event } => {
                    let meta_pass_event = GenericKvLogEvent::MetaPass(MetaPassObject::Update { event: event.clone() });
                    let save_command = self.repo.save_event(&meta_pass_event).await;

                    if save_command.is_err() {
                        let err_msg = String::from("Error saving meta pass request");
                        self.logger.info(err_msg.as_str());
                        panic!("Error");
                    }
                }
            },
            GenericKvLogEvent::Mempool(evt_type) => {
                // save mempool event in the database
                self.logger.info("Data Sync. Handle mem pool request");
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

            GenericKvLogEvent::SharedSecret(sss_obj) => {
                let obj_desc = generic_event.key().obj_desc();

                let slot_id = self.persistent_obj
                    .find_tail_id_by_obj_desc(&obj_desc)
                    .await
                    .map(|id| id.next())
                    .unwrap_or(ObjectId::unit(&obj_desc));

                let shared_secret_event = match sss_obj {
                    SharedSecretObject::Split { event } => {
                        GenericKvLogEvent::SharedSecret(SharedSecretObject::Split {
                            event: KvLogEvent {
                                key: KvKey::Empty { obj_desc },
                                value: event.value.clone(),
                            },
                        })
                    }
                    SharedSecretObject::Recover { event } => {
                        GenericKvLogEvent::SharedSecret(SharedSecretObject::Recover {
                            event: KvLogEvent {
                                key: KvKey::Empty { obj_desc },
                                value: event.value.clone(),
                            }
                        })
                    }
                    SharedSecretObject::RecoveryRequest { event } => {
                       GenericKvLogEvent::SharedSecret(SharedSecretObject::RecoveryRequest {
                            event: KvLogEvent {
                                key: KvKey::Empty { obj_desc },
                                value: event.value.clone(),
                            },
                        })
                    }
                };

                let _ = self.repo.save(&slot_id, &shared_secret_event).await;
            }

            GenericKvLogEvent::LocalEvent(evt_type) => {
                self.logger
                    .info(format!("Local events can't be sent: {:?}", evt_type).as_str());
            }
            GenericKvLogEvent::Error { .. } => {
                self.logger.info("Errors not yet implemented");
            }
        }
    }

    async fn global_index_replication(&self, request: &SyncRequest, commit_log: &mut Vec<GenericKvLogEvent>) -> anyhow::Result<()> {
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
        Ok(())
    }

    async fn shared_secret_replication(&self, request: &SyncRequest, commit_log: &mut Vec<GenericKvLogEvent>) {
        match &request.vault_tail_id {
            None => {
                // Ignore empty vault requests
            }
            Some(_) => {
                let obj_desc = ObjectDescriptor::SharedSecret {
                    vault_name: request.sender.vault.name.clone(),
                    device_id: request.sender.vault.device.device_id.clone(),
                };
                let obj_id = ObjectId::unit(&obj_desc);

                let events = self.persistent_obj.find_object_events(&obj_id).await;
                commit_log.extend(events);
            }
        }
    }

    async fn vault_replication(&self, request: &SyncRequest, commit_log: &mut Vec<GenericKvLogEvent>) {
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
                        self.logger.info("Empty vault store");
                        vec![]
                    }
                    VaultStore::Unit { tail_id } => self.persistent_obj.get_user_sig(tail_id).await,
                    VaultStore::Genesis { tail_id, .. } => self.persistent_obj.get_user_sig(tail_id).await,
                    VaultStore::Store { vault, .. } => vault.signatures.clone(),
                };

                let vault_signatures: Vec<String> = vault_signatures
                    .iter()
                    .map(|sig| sig.public_key.base64_text.clone())
                    .collect();

                if vault_signatures.contains(&request.sender.public_key.base64_text) {
                    let vault_events = self.persistent_obj.find_object_events(vault_tail_id).await;
                    commit_log.extend(vault_events);
                } else {
                    self.logger.info(
                        format!(
                            "The client is not a member of the vault. Client pk: {:?}, vault: {:?}",
                            &request.sender, meta_db.vault_store
                        )
                            .as_str(),
                    );
                    self.logger.info(
                        format!(
                            "Vault sigs: {:?}, sender sig: {:?}",
                            vault_signatures, &request.sender.public_key.base64_text
                        )
                            .as_str(),
                    );
                }
            }
        }
    }

    async fn meta_pass_replication(&self, request: &SyncRequest, commit_log: &mut Vec<GenericKvLogEvent>) {
        match &request.meta_pass_tail_id {
            None => {
                // Ignore empty requests
            }
            Some(meta_pass_tail_id) => {
                let meta_pass_events = self.persistent_obj.find_object_events(meta_pass_tail_id).await;
                commit_log.extend(meta_pass_events);
            }
        }
    }
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> DataSync<Repo, Logger> {
    async fn accept_sign_up_request(&self, event: &KvLogEvent<UserSignature>, vault_id: &IdStr) {
        //vault not found, we can create our new vault
        self.logger.info("Accept SignUp request");

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
            ObjectId::Genesis { .. } => ObjectId::global_index_genesis().next(),
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
pub mod test {
    use super::*;
    use crate::models::DeviceInfo;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use std::rc::Rc;
    use crate::node::logger::{DefaultMetaLogger, LoggerId};

    #[tokio::test]
    async fn test_accept_sign_up() {
        let ctx = DataSyncTestContext::new();
        let data_sync = ctx.data_sync;

        let vault_unit = GenericKvLogEvent::Vault(VaultObject::unit(&ctx.user_sig));
        data_sync.send(&vault_unit).await;

        let request = SyncRequest {
            sender: ctx.user_sig.as_ref().clone(),
            vault_tail_id: Some(ObjectId::vault_unit("test_vault")),
            meta_pass_tail_id: None,
            global_index: None,
        };
        let events = data_sync.replication(request).await.unwrap();

        match &events[0] {
            GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Unit { event }) => {
                match &event.key {
                    KvKey::Empty { .. } => {
                        panic!()
                    }
                    KvKey::Key { obj_id, .. } => {
                        assert!(obj_id.is_unit());
                    }
                }
            }
            _ => panic!("Invalid event")
        }

        match &events[1] {
            GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Genesis { event }) => {
                match &event.key {
                    KvKey::Empty { .. } => {
                        panic!()
                    }
                    KvKey::Key { obj_id, .. } => {
                        assert!(obj_id.is_genesis());
                    }
                }
            }
            _ => panic!("Invalid event")
        }

        match &events[2] {
            GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Update { event }) => {
                match event.key.clone() {
                    KvKey::Empty { .. } => {
                        panic!()
                    }
                    KvKey::Key { obj_id, .. } => {
                        assert_eq!(obj_id.unit_id().next().next(), obj_id);
                    }
                }
            }
            _ => panic!("Invalid event")
        }

        match &events[3] {
            GenericKvLogEvent::Vault(VaultObject::Unit { event }) => {
                match event.key.clone() {
                    KvKey::Empty { .. } => {
                        panic!()
                    }
                    KvKey::Key { obj_id, .. } => {
                        assert!(obj_id.is_unit());
                    }
                }
            }
            _ => panic!("Invalid event")
        }

        match &events[4] {
            GenericKvLogEvent::Vault(VaultObject::Genesis { event }) => {
                match event.key.clone() {
                    KvKey::Empty { .. } => {
                        panic!()
                    }
                    KvKey::Key { obj_id, .. } => {
                        assert!(obj_id.is_genesis());
                    }
                }
            }
            _ => panic!("Invalid event")
        }

        match &events[5] {
            GenericKvLogEvent::Vault(VaultObject::SignUpUpdate { event }) => {
                match event.key.clone() {
                    KvKey::Empty { .. } => {
                        panic!()
                    }
                    KvKey::Key { obj_id, .. } => {
                        assert_eq!(obj_id.unit_id().next().next(), obj_id);
                    }
                }
            }
            _ => panic!("Invalid event")
        }
    }

    pub struct DataSyncTestContext {
        pub logger: Rc<DefaultMetaLogger>,
        pub repo: Rc<InMemKvLogEventRepo>,
        pub persistent_obj: Rc<PersistentObject<InMemKvLogEventRepo, DefaultMetaLogger>>,
        pub meta_db_manager: Rc<MetaDbManager<InMemKvLogEventRepo, DefaultMetaLogger>>,
        pub data_sync: DataSync<InMemKvLogEventRepo, DefaultMetaLogger>,
        pub user_sig: Rc<UserSignature>,
        pub user_creds: Rc<UserCredentials>,
    }

    impl DataSyncTestContext {
        pub fn new() -> Self {
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
            let user_creds = Rc::new(UserCredentials {
                security_box: Box::new(s_box),
                user_sig: Box::new(user_sig.clone()),
            });

            let data_sync = DataSync {
                persistent_obj: persistent_object.clone(),
                repo: repo.clone(),
                context: Rc::new(MetaServerContextState::from(user_creds.as_ref())),
                meta_db_manager: meta_db_manager.clone(),
                logger: logger.clone(),
            };

            Self {
                logger,
                repo,
                persistent_obj: persistent_object,
                meta_db_manager,
                data_sync,
                user_sig: Rc::new(user_sig),
                user_creds,
            }
        }
    }
}