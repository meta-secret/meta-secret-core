use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, error, info, instrument, Instrument};

use crate::node::common::model::device::DeviceCredentials;
use crate::node::db::events::common::SharedSecretObject;
use crate::node::db::events::db_tail::{DbTail};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::DbTailObject;
use crate::node::db::events::object_descriptor::{ObjectDescriptor};
use crate::node::db::events::object_descriptor::global_index::GlobalIndexDescriptor;
use crate::node::db::events::object_id::{Next, ObjectId, UnitId};
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::read_db::read_db_service::ReadDbServiceProxy;
use crate::node::db::read_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::data_sync::DataSyncMessage;
use crate::node::server::request::SyncRequest;
use crate::node::server::server_app::ServerDataTransfer;

pub struct SyncGateway<Repo: KvLogEventRepo> {
    pub id: String,
    pub persistent_object: Arc<PersistentObject<Repo>>,
    pub server_dt: Arc<ServerDataTransfer>,
    pub read_db_service_proxy: Arc<ReadDbServiceProxy>,
    pub device_creds: DeviceCredentials
}

impl<Repo: KvLogEventRepo> SyncGateway<Repo> {
    #[instrument(skip_all)]
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Run sync gateway");

        loop {
            self.sync().await?;
            async_std::task::sleep(Duration::from_millis(300)).await;
        }
    }

    ///First level of synchronization
    ///  - global index, server PK - when user has no account
    ///  - vault, meta pass... - user has been registered
    ///
    #[instrument(skip_all)]
    pub async fn sync(&self) -> anyhow::Result<()> {

        let vault_name = client_creds.user_sig.vault.name.as_str();
        let db_tail_result = self.persistent_object.get_db_tail(vault_name).in_current_span().await;

        let Ok(db_tail) = db_tail_result else {
            error!("Error! Db tail not exists");
            return Ok(());
        };

        let new_gi_tail = self.get_new_tail_for_global_index(&db_tail).in_current_span().await;

        let new_vault_tail_id = self.get_new_tail_for_an_obj(&db_tail.vault_id).in_current_span().await;

        let new_meta_pass_tail_id = self
            .get_new_tail_for_an_obj(&db_tail.meta_pass_id)
            .in_current_span()
            .await;

        let new_mem_pool_tail_id = self.get_new_tail_for_mem_pool(&db_tail).in_current_span().await;

        let new_audit_tail = self.sync_shared_secrets(vault_name, &client_creds, &db_tail).await;

        let new_db_tail = DbTail {
            vault_id: new_vault_tail_id,
            meta_pass_id: new_meta_pass_tail_id,

            global_index_id: new_gi_tail,
            mem_pool_id: new_mem_pool_tail_id.clone(),
            s_s_audit: new_audit_tail.clone(),
        };

        self.save_updated_db_tail(db_tail, new_db_tail.clone())
            .in_current_span()
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
            let key = self.persistent_object.repo.save(new_event.clone()).in_current_span().await?;

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
                },
                GenericKvLogEvent::Vault(_) => {
                    latest_vault_id = ObjectIdDbEvent::Id { tail_id: key.clone() }
                },
                GenericKvLogEvent::MetaPass(_) => {
                    latest_meta_pass_id = ObjectIdDbEvent::Id { tail_id: key.clone() }
                },
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

    async fn get_new_tail_for_an_obj(&self, db_tail_obj: &ObjectIdDbEvent) -> ObjectIdDbEvent {
        match db_tail_obj {
            ObjectIdDbEvent::Empty { obj_desc } => self
                .persistent_object
                .find_tail_id(unit_id.clone())
                .await
                .map(|tail_id| ObjectIdDbEvent::Id { tail_id })
                .unwrap_or(ObjectIdDbEvent::Empty {
                    unit_id: unit_id.clone(),
                }),
            ObjectIdDbEvent::Id { tail_id } => {
                let tail_id_sync = match tail_id {
                    ObjectId::Unit { .. } => tail_id.clone(),
                    _ => tail_id.next(),
                };

                let obj_events = self.persistent_object.find_object_events(&tail_id_sync).await;
                let last_vault_event = obj_events.last().cloned();

                for client_event in obj_events {
                    debug!(
                        "Send event to server. May stuck if server won't response!!! : {:?}",
                        client_event
                    );
                    self.server_dt
                        .dt
                        .send_to_service(DataSyncMessage::Event(client_event))
                        .await;
                }

                let new_tail_id = last_vault_event
                    .map(|event| event.key().obj_id.clone())
                    .unwrap_or(tail_id.clone());

                ObjectIdDbEvent::Id { tail_id: new_tail_id }
            }
        }
    }

    async fn get_new_tail_for_global_index(&self, db_tail: &DbTail) -> Option<ObjectId> {
        let global_index = db_tail
            .global_index_id
            .clone()
            .unwrap_or(ObjectId::from(UnitId::global_index()));

        self.persistent_object.find_tail_id(global_index).await
    }

    async fn get_new_tail_for_mem_pool(&self, db_tail: &DbTail) -> Option<ObjectId> {
        let mem_pool_id = match db_tail.mem_pool_id.clone() {
            None => ObjectId::mempool_unit(),
            Some(obj_id) => obj_id.next(),
        };

        let mem_pool_events = self.persistent_object.find_object_events(&mem_pool_id).await;
        let last_pool_event = mem_pool_events.last().cloned();

        for client_event in mem_pool_events {
            debug!("send mem pool request to server: {:?}", client_event);
            self.server_dt
                .dt
                .send_to_service(DataSyncMessage::Event(client_event))
                .await;
        }

        match last_pool_event {
            None => db_tail.mem_pool_id.clone(),
            Some(event) => Some(event.key().obj_id.clone()),
        }
    }
}
