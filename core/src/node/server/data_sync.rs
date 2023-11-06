use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, error, info, instrument, Instrument};

use crate::crypto::key_pair::KeyPair;
use crate::node::common::model::device::{DeviceCredentials, DeviceData};
use crate::node::common::model::user::{UserData, UserDataCandidate};
use crate::node::db::actions::join;
use crate::node::db::actions::sign_up::SignUpAction;
use crate::node::db::actions::ss_replication::SharedSecretReplicationAction;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_descriptor::global_index::GlobalIndexDescriptor;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::read_db::read_db_service::ReadDbServiceProxy;
use crate::node::db::read_db::store::vault_store::VaultStore;
use crate::node::server::request::SyncRequest;

#[async_trait(? Send)]
pub trait DataSyncApi {
    async fn replication(&self, request: SyncRequest) -> anyhow::Result<Vec<GenericKvLogEvent>>;
    async fn send(&self, event: GenericKvLogEvent);
}

pub struct ServerDataSync<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
    pub device_creds: DeviceCredentials,
    read_db_service_proxy: Arc<ReadDbServiceProxy>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncRequest {
    TailRequest {
        sender: UserData,
    },
    SyncRequest(SyncRequest),
    Event(GenericKvLogEvent),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncResponse {
    Tail {
        vault_audit_id: ObjectId
    },
    Data {
        events: Vec<GenericKvLogEvent>
    }
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo> DataSyncApi for ServerDataSync<Repo> {

    #[instrument(skip(self))]
    async fn replication(&self, request: SyncRequest) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        match request {
            SyncRequest::GlobalIndex { global_index, .. } => {
                let gi_events = self.global_index_replication(global_index)
                    .await?;
                commit_log.extend(gi_events);
            }
            SyncRequest::Vault { request, .. } => {
                let vault_events = self.vault_replication(&request)
                    .in_current_span()
                    .await;
                commit_log.extend(vault_events);

                let meta_pass_events = self.meta_pass_replication(&request)
                    .in_current_span()
                    .await;
                commit_log.extend(meta_pass_events);

                let s_s_replication_action = SharedSecretReplicationAction {
                    persistent_obj: self.persistent_obj.clone(),
                };

                let s_s_replication_events = s_s_replication_action.replicate(&request).in_current_span().await;
                commit_log.extend(s_s_replication_events);
            }
        }

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled and the actions will be executed accordingly
    async fn send(&self, generic_event: GenericKvLogEvent) {
        self.server_processing(generic_event).in_current_span().await;
    }
}

impl<Repo: KvLogEventRepo> ServerDataSync<Repo> {
    pub async fn new(
        creds: DeviceCredentials,
        persistent_obj: Arc<PersistentObject<Repo>>,
        read_db_service_proxy: Arc<ReadDbServiceProxy>,
    ) -> Self {
        ServerDataSync {
            persistent_obj: persistent_obj.clone(),
            device_creds: creds,
            read_db_service_proxy,
        }
    }
}

impl<Repo: KvLogEventRepo> ServerDataSync<Repo> {
    #[instrument(skip_all)]
    async fn server_processing(&self, generic_event: GenericKvLogEvent) {
        debug!("DataSync::event_processing: {:?}", &generic_event);

        match &generic_event {
            GenericKvLogEvent::GlobalIndex(_) => {
                info!("Global index not allowed to be sent");
            }

            GenericKvLogEvent::Vault(vault_obj_info) => {
                match vault_obj_info {
                    VaultObject::Unit { event } => {
                        info!("Handle 'vault_object:unit' event");
                        // Handled by the server. Add a vault to the system
                        let vault_id = event.key.obj_id.clone();

                        info!("Looking for a vault: {}", vault_id.id_str());

                        let vault_formation_event_result = self
                            .persistent_obj
                            .repo
                            .find_one(vault_id.clone())
                            .in_current_span()
                            .await;

                        let vault_id_str = IdStr::from(&vault_id);

                        match vault_formation_event_result {
                            Err(_) => {
                                self.accept_sign_up_request(event, &vault_id_str)
                                    .in_current_span()
                                    .await;
                            }
                            Ok(maybe_sign_up) => match maybe_sign_up {
                                None => {
                                    self.accept_sign_up_request(event, &vault_id_str).await;
                                }
                                Some(_sign_up) => {
                                    info!("Error. Vault already exists. Skip");
                                }
                            },
                        }
                    }
                    VaultObject::Genesis { .. } => {
                        info!("Genesis event not allowed to send. Skip");
                    }
                    VaultObject::JoinUpdate { .. } => {
                        let _ = self.persistent_obj.repo.save(generic_event).await;
                    }
                    VaultObject::JoinRequest { .. } => {
                        //self.logger.log("Handle join request");
                        //self.accept_join_cluster_request(event).await;
                        info!("Ignore Join request on server side");
                    }
                }
            }
            GenericKvLogEvent::MetaPass(meta_pass_obj) => match meta_pass_obj {
                MetaPassObject::Unit { .. } => {
                    info!("Ignore unit event for meta pass");
                }
                MetaPassObject::Genesis { .. } => {
                    info!("Ignore genesis event for meta pass");
                }
                MetaPassObject::Update { event } => {
                    let meta_pass_event = GenericKvLogEvent::MetaPass(MetaPassObject::Update { event: event.clone() });
                    let save_command = self.persistent_obj.repo.save(meta_pass_event).await;

                    if save_command.is_err() {
                        let err_msg = String::from("Error saving meta pass request");
                        info!(err_msg);
                        error!("Error");
                    }
                }
            },
            GenericKvLogEvent::MemPool(evt_type) => {
                // save mem pool event in the database
                info!("Data Sync. Handle mem pool request");
                match evt_type {
                    MemPoolObject::JoinRequest { event } => {
                        let vault_name = event.value.vault.name.clone();
                        let vault_obj_id = ObjectId::vault_unit(vault_name.as_str());

                        let maybe_vault_tail_id = self.persistent_obj.find_tail_id(vault_obj_id).await;

                        match maybe_vault_tail_id {
                            None => {
                                //ignore, vault not exists yet, no way to join vault
                            }
                            Some(vault_tail_id) => {
                                let join_request = GenericKvLogEvent::Vault(VaultObject::JoinRequest {
                                    event: join::join_cluster_request(&vault_tail_id, &event.value),
                                });

                                let _ = self.persistent_obj.repo.save(join_request).await;
                            }
                        }
                    }
                }
            }

            GenericKvLogEvent::SharedSecret(_) => {
                let _ = self.persistent_obj.repo.save(generic_event.clone()).await;
            }
            GenericKvLogEvent::Error { .. } => {
                info!("Errors not yet implemented");
            }
            GenericKvLogEvent::Credentials(evt_type) => {
                info!("Local events can't be sent: {:?}", evt_type);
            }
            GenericKvLogEvent::DbTail(evt_type) => {
                info!("Local events can't be sent: {:?}", evt_type);
            }
        }
    }

    #[instrument(skip_all)]
    async fn global_index_replication(&self, gi_id: ObjectId) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let events = self.persistent_obj
            .find_object_events(gi_id)
            .await?;
        Ok(events)
    }

    pub async fn vault_replication(&self, request: &SyncRequest) -> Vec<GenericKvLogEvent> {
        let events = match &request.vault_tail_id {
            None => {
                vec![]
            }
            Some(vault_tail_id) => {
                //sync meta db!!! how? See MetaDbService::sync_db()
                let vault_store = self
                    .read_db_service_proxy
                    .get_vault_store(request.sender.vault.name.clone())
                    .await
                    .unwrap();

                let vault_signatures = match &vault_store {
                    VaultStore::Empty => {
                        info!("Empty vault store");
                        vec![]
                    }
                    VaultStore::Unit { id: tail_id } => self.persistent_obj.get_vault_unit_sig(tail_id.clone()).await,
                    VaultStore::Genesis { id: tail_id, .. } => self.persistent_obj.get_vault_unit_sig(tail_id.clone()).await,
                    VaultStore::Store { vault, .. } => vault.signatures.clone(),
                };

                let vault_signatures: Vec<String> = vault_signatures
                    .iter()
                    .map(|sig| sig.public_key.base64_text.clone())
                    .collect();

                if vault_signatures.contains(&request.sender.public_key.base64_text) {
                    let vault_events = self.persistent_obj.find_object_events(vault_tail_id).await;
                    vault_events
                } else {
                    debug!(
                        "The client is not a member of the vault.\nRequest: {:?},\nvault store: {:?}",
                        &request, &vault_store
                    );
                    vec![]
                }
            }
        };

        events
    }

    async fn meta_pass_replication(&self, request: &SyncRequest) -> Vec<GenericKvLogEvent> {
        match &request.meta_pass_tail_id {
            None => {
                vec![]
            }
            Some(meta_pass_tail_id) => {
                self.persistent_obj.find_object_events(meta_pass_tail_id).await
            }
        }
    }
}

impl<Repo: KvLogEventRepo> ServerDataSync<Repo> {
    async fn accept_sign_up_request(&self, event: &KvLogEvent<UserDataCandidate>, vault_id: &IdStr) {
        //vault not found, we can create our new vault
        info!("Accept SignUp request, for the vault: {:?}", vault_id);

        let server_pk = PublicKeyRecord::from(self.device_creds.device.keys.dsa_pk.clone());
        let sign_up_action = SignUpAction {};
        let sign_up_events = sign_up_action.accept(event, &server_pk);

        for sign_up_event in sign_up_events {
            self.persistent_obj
                .repo
                .save(sign_up_event)
                .await
                .expect("Error saving sign_up events");
        }

        self.update_global_index(vault_id, &server_pk).await;
    }

    async fn update_global_index(&self, vault_id: &IdStr, server_pk: &PublicKeyRecord) {
        //find the latest global_index_id???
        let gi_obj_id = ObjectId::unit(&ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index));
        let global_index_tail_id = self
            .persistent_obj
            .find_tail_id(gi_obj_id.clone())
            .await
            .unwrap_or(gi_obj_id);

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
            ObjectId::Artifact { .. } => global_index_tail_id.next(),
        };

        let gi_update_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Update {
            event: KvLogEvent::new_global_index_event(&gi_obj_id, vault_id, GlobalIndexDescriptor::Index),
        });

        gi_events.push(gi_update_event);

        for gi_event in gi_events {
            self.persistent_obj
                .repo
                .save(gi_event)
                .await
                .expect("Error saving vaults genesis event");
        }

        let vault_idx_unit_id = ObjectId::unit(&ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::VaultIndex));
        let vault_idx_evt = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::VaultIndex {
            event: KvLogEvent::new_global_index_event(&vault_idx_unit_id, vault_id, GlobalIndexDescriptor::VaultIndex),
        });

        self.persistent_obj.repo.save(vault_idx_evt).await.expect("error");
    }
}


#[cfg(test)]
pub mod test {
    use crate::node::common::data_transfer::MpscDataTransfer;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::read_db::read_db_service::{ReadDbDataTransfer, ReadDbService};

    use super::*;

    /// Disabled. Reason: DataSyncTestContext has to start MetaDbService as a separate task, otherwise test get stuck
    /// because the service has been stopped
    #[ignore]
    #[tokio::test]
    async fn test_accept_sign_up() {
        let ctx = DataSyncTestContext::default();
        let data_sync = ctx.data_sync;

        let vault_unit = GenericKvLogEvent::Vault(VaultObject::unit(&ctx.user_sig));
        data_sync.send(vault_unit).await;

        let request = SyncRequest {
            sender: ctx.user_sig.as_ref().clone(),
            vault_tail_id: Some(ObjectId::vault_unit("test_vault")),
            meta_pass_tail_id: None,
            global_index: None,
            s_s_audit: None,
        };
        let events = data_sync.replication(request).await.unwrap();

        match &events[0] {
            GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Unit {
                                               event: KvLogEvent { key, .. },
                                           }) => {
                assert!(key.obj_id.is_unit());
            }
            _ => panic!("Invalid event"),
        }

        match &events[1] {
            GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Genesis {
                                               event: KvLogEvent { key, .. },
                                           }) => {
                assert!(key.obj_id.is_genesis());
            }
            _ => panic!("Invalid event"),
        }

        match &events[2] {
            GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Update {
                                               event: KvLogEvent { key, .. },
                                           }) => {
                assert_eq!(key.obj_id.unit_id().next().next(), key.obj_id);
            }
            _ => panic!("Invalid event"),
        }

        match &events[3] {
            GenericKvLogEvent::Vault(VaultObject::Unit {
                                         event: KvLogEvent { key, .. },
                                     }) => {
                assert!(key.obj_id.is_unit());
            }
            _ => panic!("Invalid event"),
        }

        match &events[4] {
            GenericKvLogEvent::Vault(VaultObject::Genesis {
                                         event: KvLogEvent { key, .. },
                                     }) => {
                assert!(key.obj_id.is_genesis());
            }
            _ => panic!("Invalid event"),
        }

        match &events[5] {
            GenericKvLogEvent::Vault(VaultObject::SignUpUpdate {
                                         event: KvLogEvent { key, .. },
                                     }) => {
                assert_eq!(key.obj_id.unit_id().next().next(), key.obj_id);
            }
            _ => panic!("Invalid event"),
        }
    }

    pub struct DataSyncTestContext {
        pub repo: Arc<InMemKvLogEventRepo>,
        pub persistent_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,
        pub read_db_service: Arc<ReadDbService<InMemKvLogEventRepo>>,
        pub data_sync: ServerDataSync<InMemKvLogEventRepo>,
        pub user_sig: Arc<UserSignature>,
        pub user_creds: UserCredentials,
    }

    impl Default for DataSyncTestContext {
        fn default() -> Self {
            let repo = Arc::new(InMemKvLogEventRepo::default());

            let persistent_object = Arc::new(PersistentObject::new(repo.clone()));

            let read_db_dt = Arc::new(ReadDbDataTransfer {
                dt: MpscDataTransfer::new(),
            });

            let client_read_db_service = Arc::new(ReadDbService {
                persistent_obj: persistent_object.clone(),
                repo: persistent_object.repo.clone(),
                read_db_id: String::from("test"),
                data_transfer: read_db_dt.clone(),
            });

            let s_box = KeyManager::generate_secret_box("test_vault".to_string());
            let device = DeviceInfo {
                device_id: "a".to_string(),
                device_name: "a".to_string(),
            };
            let user_sig = s_box.get_user_sig(&device);
            let user_creds = UserCredentials {
                security_box: s_box,
                user_sig: user_sig.clone(),
            };

            let data_sync = ServerDataSync {
                persistent_obj: persistent_object.clone(),
                context: Arc::new(MetaServerContextState::from(&user_creds)),
                read_db_service_proxy: Arc::new(ReadDbServiceProxy { dt: read_db_dt }),
            };

            Self {
                repo,
                persistent_obj: persistent_object,
                read_db_service: client_read_db_service,
                data_sync,
                user_sig: Arc::new(user_sig),
                user_creds,
            }
        }
    }
}
