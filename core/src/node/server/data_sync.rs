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
use crate::node::db::meta_db::MetaDb;
use crate::node::db::models::{GenericKvLogEvent, MempoolObject, MetaPassObject, PublicKeyRecord};
use crate::node::db::models::{GlobalIndexObject, KvLogEvent, ObjectCreator, ObjectDescriptor, VaultObject};
use crate::node::server::persistent_object::PersistentObject;
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
    async fn sync_data<L: MetaLogger>(&self, request: SyncRequest, logger: &L) -> Result<Vec<GenericKvLogEvent>, Err>;
    async fn send_data<L: MetaLogger>(&self, event: &GenericKvLogEvent, logger: &L);
}

pub struct DataSync<Repo: KvLogEventRepo<Err>, Err: Error> {
    pub persistent_obj: Rc<PersistentObject<Repo, Err>>,
    pub repo: Rc<Repo>,
    pub context: Rc<MetaServerContextState>,
    pub meta_db_manager: Rc<MetaDbManager<Repo, Err>>,
}

//MetaServerContext
#[async_trait(? Send)]
impl<Repo: KvLogEventRepo<Err>, Err: Error> DataSyncApi<Err> for DataSync<Repo, Err> {
    async fn sync_data<L: MetaLogger>(&self, request: SyncRequest, logger: &L) -> Result<Vec<GenericKvLogEvent>, Err> {
        //logger.log("sync data");

        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        let mut meta_db = MetaDb::default();
        meta_db.vault_store.tail_id = request.vault_tail_id.clone().map(|id| id.unit_id());

        match request.global_index {
            None => {
                let meta_g = self
                    .persistent_obj
                    .get_object_events_from_beginning(&ObjectDescriptor::GlobalIndex, &self.context.server_pk(), logger)
                    .await?;
                commit_log.extend(meta_g);
            }
            Some(index_id) => {
                let meta_g = self.persistent_obj.find_object_events(&index_id, logger).await;
                commit_log.extend(meta_g);
            }
        }

        match &request.vault_tail_id {
            None => {
                // Ignore empty requests
            }
            Some(vault_tail_id) => {
                let meta_db = self.meta_db_manager.sync_meta_db(meta_db, logger).await;

                match meta_db.vault_store.vault {
                    None => {
                        logger.log("Error!!! Vault is not set");
                        //panic!("Impossible")
                    }
                    Some(server_vault) => {
                        let signatures: Vec<String> = server_vault
                            .signatures
                            .iter()
                            .map(|sig| sig.transport_public_key.base64_text.to_string())
                            .collect();

                        if signatures.contains(&request.sender_pk.pk.base64_text) {
                            let vault_events = self.persistent_obj.find_object_events(vault_tail_id, logger).await;
                            commit_log.extend(vault_events);
                        }
                    }
                }
            }
        }

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled and the actions will be executed accordingly
    async fn send_data<L: MetaLogger>(&self, generic_event: &GenericKvLogEvent, logger: &L) {
        logger.log("DataSync::send_data");

        match generic_event {
            GenericKvLogEvent::GlobalIndex(_) => {
                logger.log("Global index not allowed to be sent");
            }
            GenericKvLogEvent::Vault(vault_obj_info) => {
                match vault_obj_info {
                    VaultObject::Unit { event } => {
                        logger.log("Handle 'vault_object:unit' event");
                        // Handled by the server. Add a vault to the system
                        let vault_id = event.key.obj_id.unit_id();

                        logger.log(format!("Looking for a vault: {}", vault_id.id_str()).as_str());

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
                                    logger.log("Error. Vault already exists. Skip");
                                }
                            },
                        }
                    }
                    VaultObject::Genesis { .. } => {
                        logger.log("Genesis event not allowed to send. Skip");
                    }
                    VaultObject::SignUpUpdate { .. } => {
                        logger.log("SignUp update not allowed to send. Skip");
                    }
                    VaultObject::JoinUpdate { .. } => {
                        logger.log("Join with update not allowed to send. Skip");
                    }
                    VaultObject::JoinRequest { event } => {
                        logger.log("Handle join request");
                        let user_sig: UserSignature = event.value.clone();
                        let obj_desc = ObjectDescriptor::Vault {
                            vault_name: user_sig.vault.name,
                        };
                        let vault_id = ObjectId::unit(&obj_desc);
                        self.accept_join_cluster_request(event, &vault_id, logger).await;
                    }
                }
            }
            GenericKvLogEvent::MetaPass(_) => {
                //TODO save meta pass!!!
            }
            GenericKvLogEvent::Mempool(evt_type) => {
                // save mempool event in the database
                logger.log("Dat Sync. Handle mem pool request");
                match evt_type {
                    MempoolObject::JoinRequest { event } => {
                        let vault_name = event.value.vault.name.clone();
                        let vault_obj_id = ObjectId::vault_unit(vault_name.as_str());

                        let tail_id = self.persistent_obj.find_tail_id(&vault_obj_id).await.unwrap();

                        let join_request = GenericKvLogEvent::Vault(VaultObject::JoinRequest {
                            event: join::join_cluster_request(&tail_id, &event.value),
                        });

                        self.send_data(&join_request, logger).await;
                    }
                }
            }
            GenericKvLogEvent::LocalEvent(evt_type) => {
                logger.log(format!("Local events can't be sent: {:?}", evt_type).as_str());
            }
            GenericKvLogEvent::Error { .. } => {
                logger.log("Errors not yet implemented");
            }
        }
    }
}

impl<Repo: KvLogEventRepo<Err>, Err: Error> DataSync<Repo, Err> {
    async fn accept_join_cluster_request<L: MetaLogger>(
        &self,
        join_event: &KvLogEvent<UserSignature>,
        obj_id: &ObjectId,
        logger: &L,
    ) {
        logger.log(format!("save join request: {}", serde_json::to_string(&join_event).unwrap()).as_str());

        let generic_join_event = GenericKvLogEvent::Vault(VaultObject::JoinRequest {
            event: join_event.clone(),
        });
        self.repo
            .save_event(&generic_join_event)
            .await
            .expect("Error saving join request");

        //join cluster update message
        let vault_events = self.persistent_obj.find_object_events(&obj_id.unit_id(), logger).await;

        let meta_db = self.meta_db_manager.transform(vault_events);

        let generic_accept_event = {
            let vault_doc = &meta_db.unwrap().vault_store.vault.unwrap();

            GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                event: join::accept_join_request(join_event, vault_doc),
            })
        };

        self.repo
            .save_event(&generic_accept_event)
            .await
            .expect("Error saving accept event");
    }

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
