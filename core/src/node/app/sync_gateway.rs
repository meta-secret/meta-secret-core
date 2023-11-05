use std::sync::Arc;
use std::time::Duration;
use anyhow::anyhow;

use tracing::{debug, error, info, instrument, Instrument};
use crate::node::common::model::device::{DeviceCredentials, DeviceData};

use crate::node::db::events::common::SharedSecretObject;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::generic_log_event::{ToGenericEvent, GenericKvLogEvent, KeyExtractor, ObjIdExtractor};
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::{CredentialsObject, DbTailObject};
use crate::node::db::events::object_descriptor::{ObjectDescriptor, VaultDescriptor};
use crate::node::db::events::object_descriptor::global_index::GlobalIndexDescriptor;
use crate::node::db::events::object_id::{Next, ObjectId, UnitId};
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::read_db::read_db_service::ReadDbServiceProxy;
use crate::node::db::read_db::store::vault_store::VaultStore;
use crate::node::server::data_sync::DataSyncMessage;
use crate::node::server::request::{VaultRequest, GlobalIndexRequest, SyncRequest};
use crate::node::server::server_app::ServerDataTransfer;

pub struct SyncGateway<Repo: KvLogEventRepo> {
    pub id: String,
    pub persistent_object: Arc<PersistentObject<Repo>>,
    pub server_dt: Arc<ServerDataTransfer>,
    pub read_db_service_proxy: Arc<ReadDbServiceProxy>,
    pub creds: CredentialsObject,
}

impl<Repo: KvLogEventRepo> SyncGateway<Repo> {
    #[instrument(skip_all)]
    pub async fn run(&self) {
        info!("Run sync gateway");

        loop {
            let result = self.sync().await;
            if let Err(err) = result {
                error!("Sync error: {:?}", err);
            }

            async_std::task::sleep(Duration::from_millis(300)).await;
        }
    }

    ///First level of synchronization
    ///  - global index, server PK - when user has no account
    ///  - vault, meta pass... - user has been registered
    ///
    #[instrument(skip_all)]
    pub async fn sync(&self) -> anyhow::Result<()> {
        let db_tail = self.persistent_object.get_db_tail().await?;

        {
            let sender = match &self.creds {
                CredentialsObject::Device { event } => event.value.device.clone(),
                CredentialsObject::User { event } => event.value.device_creds.device.clone()
            };
            self.sync_global_index(sender).await?;
        }

        if let CredentialsObject::User { event } = &self.creds {
            let user_creds = &event.value;
            let sender = event.value.device();

            let obj_desc = ObjectDescriptor::Vault(VaultDescriptor::Vault {
                vault_name: event.value.vault_name.clone() }
            );
            let vault_free_id = self.persistent_object.find_free_id_by_obj_desc(obj_desc).await?;
            ObjectDescriptor::SharedSecret(SharedSec)

            let sync_request = SyncRequest::Vault {
                sender,
                request: VaultRequest {
                    vault_tail_id: vault_free_id,
                    meta_pass_tail_id: ,
                    s_s_audit: (),
                },
            };

            let new_meta_pass_tail_id = self
                .get_new_tail_for_an_obj(&db_tail.meta_pass_id)
                .await;
        }

        //let user_data = self.persistent_object.get_vault_unit_sig().await;
        //let new_gi_tail = self.get_free_tail_id_for_global_index(&db_tail).await?;

        let new_audit_tail = self.sync_shared_secrets(vault_name, &client_creds, &db_tail).await;

        let new_db_tail = DbTail {
            vault_id: new_vault_tail_id,
            meta_pass_id: new_meta_pass_tail_id,

            global_index_id: new_gi_tail,
            mem_pool_id: new_mem_pool_tail_id.clone(),
            s_s_audit: new_audit_tail.clone(),
        };

        self.save_updated_db_tail(db_tail, new_db_tail.clone())
            .await?;

        let sync_request = {
            let vault_id_request = match &new_db_tail.vault_id {
                ObjectIdDbEvent::Empty { obj_desc } => ObjectId::unit(obj_desc),
                ObjectIdDbEvent::Id { tail_id } => tail_id.next(),
            };

            let meta_pass_id_request = match &new_db_tail.meta_pass_id {
                ObjectIdDbEvent::Empty { obj_desc } => ObjectId::unit(obj_desc),
                ObjectIdDbEvent::Id { tail_id } => tail_id.next(),
            };

            SyncRequest {
                sender: client_creds.user_sig.clone(),
                global_index: new_db_tail.global_index_id.clone().map(|gi| gi.next()),
                vault_tail_id: Some(vault_id_request),
                meta_pass_tail_id: Some(meta_pass_id_request),
                s_s_audit: new_audit_tail.clone(),
            }
        };

        let mut latest_gi = new_db_tail.global_index_id.clone();
        let mut latest_vault_id = new_db_tail.vault_id.clone();
        let mut latest_meta_pass_id = new_db_tail.meta_pass_id.clone();
        let mut latest_audit_tail = new_audit_tail.clone();

        let new_server_events_res = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncMessage::SyncRequest(sync_request))
            .in_current_span()
            .await;

        let Ok(new_events) = new_server_events_res else {
            error!("DataSync error. Error loading events");
            return Ok(());
        };

        debug!("id: {:?}. Sync gateway. New events: {:?}", self.id, new_events);

        for new_event in new_events {
            let key = self.persistent_object.repo.save(new_event.clone()).await?;

            match new_event {
                GenericKvLogEvent::GlobalIndex(gi_obj) => {
                    if let GlobalIndexObject::Update { event } = gi_obj {
                        let vault_unit_id = event.value;
                        let idx_desc = ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::VaultIndex {
                            vault_id: vault_unit_id.clone()
                        });

                        let vault_idx_evt = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::VaultIndex {
                            event: KvLogEvent {
                                key: KvKey {
                                    obj_id: UnitId::unit(&idx_desc),
                                    obj_desc: idx_desc,
                                },
                                value: vault_unit_id,
                            }
                        });
                        self.persistent_object.repo.save(vault_idx_evt)
                    }

                    latest_gi = Some(key)
                }
                GenericKvLogEvent::Vault(_) => {
                    latest_vault_id = ObjectIdDbEvent::Id { tail_id: key.clone() }
                }
                GenericKvLogEvent::MetaPass(_) => {
                    latest_meta_pass_id = ObjectIdDbEvent::Id { tail_id: key.clone() }
                }
                GenericKvLogEvent::SharedSecret(SharedSecretObject::Audit { event }) => {
                    latest_audit_tail = Some(ObjectId::from(event.value))
                }
                _ => {
                    //ignore any non global event
                }
            }
        }

        let latest_db_tail = DbTail {
            vault_id: latest_vault_id,
            meta_pass_id: latest_meta_pass_id,

            global_index_id: latest_gi,
            mem_pool_id: new_mem_pool_tail_id,
            s_s_audit: latest_audit_tail,
        };

        self.save_updated_db_tail(new_db_tail.clone(), latest_db_tail).await
    }

    async fn sync_global_index(&self, sender: DeviceData) -> anyhow::Result<()> {
        //TODO optimization: read global index tail id from db_tail

        let gi_free_id = self.persistent_object
            .find_free_id_by_obj_desc(ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index))
            .await?;

        let sync_request = SyncRequest::GlobalIndex {
            sender,
            request: GlobalIndexRequest {
                global_index: gi_free_id
            },
        };

        let new_gi_events = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncMessage::SyncRequest(sync_request))
            .await?;

        for gi_event in new_gi_events {
            if let GenericKvLogEvent::GlobalIndex(gi_obj) = gi_event {
                self.persistent_object.repo.save(gi_event.clone()).await?;

                // Update vault index according to global index
                if let GlobalIndexObject::Update { event } = gi_obj {
                    let vault_id = event.value;
                    let vault_idx_evt = GlobalIndexObject::index_from(vault_id)
                        .to_generic();
                    self.persistent_object.repo.save(vault_idx_evt).await?;
                }
            } else {
                Err(anyhow!("Invalid event: {:?}", gi_event.key().obj_desc()))
            }
        }
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn sync_shared_secrets(
        &self,
        vault_name: &str,
        creds: &UserCredentials,
        db_tail: &DbTail,
    ) -> Option<ObjectId> {
        let vault_store = self
            .read_db_service_proxy
            .get_vault_store(vault_name.to_string())
            .in_current_span()
            .await
            .unwrap();

        let VaultStore::Store { vault, .. } = vault_store else {
            error!("User is not a member of the vault");
            return None;
        };

        let s_s_audit_tail = db_tail
            .s_s_audit
            .clone()
            .map(|tail_id| tail_id.next())
            .unwrap_or(ObjectId::unit(&ObjectDescriptor::SharedSecretAudit {
                vault_name: vault_name.to_string(),
            }));

        let audit_events = self
            .persistent_object
            .find_object_events(&s_s_audit_tail)
            .in_current_span()
            .await;

        let user_pk = creds.user_sig.public_key.base64_text.clone();

        for user_sig in &vault.signatures {
            if user_pk == user_sig.public_key.base64_text.clone() {
                continue;
            }

            let transfer = &self.server_dt.dt;

            for audit_event in &audit_events {
                if let GenericKvLogEvent::SharedSecret(SharedSecretObject::Audit { event }) = audit_event {
                    let ss_event_res = self.persistent_object.repo.find_one(event.value.clone()).await;

                    let Ok(Some(ss_event)) = ss_event_res else {
                        error!("Invalid event type: not an audit event");
                        continue;
                    };

                    let GenericKvLogEvent::SharedSecret(ss_obj) = &ss_event else {
                        error!("Invalid event type: not shared secret");
                        continue;
                    };

                    if let SharedSecretObject::Audit { .. } = ss_obj {
                        error!("Audit log events not allowed");
                        continue;
                    }

                    debug!("Send shared secret event to server: {:?}", event);
                    transfer
                        .send_to_service(DataSyncMessage::Event(ss_event.clone()))
                        .in_current_span()
                        .await;
                }
            }
        }

        audit_events.last().map(|evt| evt.key().obj_id.clone())
    }

    #[instrument(skip_all)]
    async fn save_updated_db_tail(&self, db_tail: DbTail, new_db_tail: DbTail) -> anyhow::Result<()> {
        if new_db_tail == db_tail {
            return Ok(());
        }

        //update db_tail
        let new_db_tail_event = GenericKvLogEvent::DbTail(DbTailObject {
            event: KvLogEvent {
                key: KvKey::unit(&ObjectDescriptor::DbTail),
                value: new_db_tail.clone(),
            },
        });

        self.persistent_object.repo.save(new_db_tail_event).await?;
        Ok(())
    }
}
