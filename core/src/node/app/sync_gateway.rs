use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use tracing::{debug, error, info, instrument, Instrument};

use crate::node::db::events::common::SharedSecretObject;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ToGenericEvent};
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::{CredentialsObject, DbTailObject};
use crate::node::db::events::object_descriptor::{ObjectDescriptor, VaultDescriptor};
use crate::node::db::events::object_descriptor::global_index::GlobalIndexDescriptor;
use crate::node::db::events::object_descriptor::shared_secret::SharedSecretDescriptor;
use crate::node::db::events::object_id::{Next, ObjectId, UnitId};
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::read_db::read_db_service::ReadDbServiceProxy;
use crate::node::db::read_db::store::vault_store::VaultStore;
use crate::node::server::data_sync::{DataSyncRequest, DataSyncResponse};
use crate::node::server::request::{SyncRequest, VaultRequest};
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

    ///Levels of synchronization:
    ///  - global index, server PK - when user has no account
    ///  - vault, shared secret... - user has been registered, we can sync vault related events
    #[instrument(skip_all)]
    pub async fn sync(&self) -> anyhow::Result<()> {
        let db_tail = self.persistent_object.get_db_tail().await?;

        self.sync_global_index().await?;

        let CredentialsObject::User { event } = &self.creds else {
            return Ok(());
        };

        //Vault synchronization
        let user_creds = &event.value;
        let sender = event.value.device();

        let server_tail = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncRequest::TailRequest {
                sender: user_creds.user()
            })
            .await?;

        let DataSyncResponse::Tail { vault_audit_tail_id } = server_tail else {
            Err(anyhow!("Invalid message from server"))
        };

        let maybe_local_audit_tail_id = self.persistent_object.repo.get_key(vault_audit_tail_id).await?;

        if let Some(local_audit_tail_id) = maybe_local_audit_tail_id {
            // Local device has more events than the server
            let local_vault_audit_events = self.persistent_object
                .find_object_events(local_audit_tail_id.next())
                .await?;

            for local_vault_audit_event in local_vault_audit_events {
                self
                    .server_dt
                    .dt
                    .send_to_service(DataSyncRequest::Event(local_vault_audit_event))
                    .await?
            }
        } else {
            // Local device has less events than the server
            let sync_request = {
                let vault_free_id = {
                    let obj_desc = VaultDescriptor::audit(event.value.vault_name.clone());
                    self.persistent_object.find_free_id_by_obj_desc(obj_desc).await?
                };

                let s_s_audit_free_id = {
                    let obj_desc = SharedSecretDescriptor::audit(event.value.vault_name.clone());
                    self.persistent_object.find_free_id_by_obj_desc(obj_desc).await?
                };

                SyncRequest::Vault {
                    sender,
                    request: VaultRequest {
                        vault_audit: vault_free_id,
                        s_s_audit: s_s_audit_free_id,
                    },
                }
            };

            let data_sync_response = self
                .server_dt
                .dt
                .send_to_service_and_get(DataSyncRequest::SyncRequest(sync_request))
                .await?;

            if let DataSyncResponse::Data { events } = data_sync_response {
                debug!("id: {:?}. Sync gateway. New events: {:?}", self.id, events);

                for new_event in events {
                    self.persistent_object.repo.save(new_event).await?;
                }
            }

            let new_audit_tail = self.sync_shared_secrets(vault_name, &client_creds, &db_tail).await;
        }

        Ok(())
    }

    async fn sync_global_index(&self) -> anyhow::Result<()> {
        //TODO optimization: read global index tail id from db_tail

        let sender = match &self.creds {
            CredentialsObject::Device { event } => event.value.device.clone(),
            CredentialsObject::User { event } => event.value.device_creds.device.clone()
        };

        let gi_free_id = self.persistent_object
            .find_free_id_by_obj_desc(ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index))
            .await?;

        let sync_request = SyncRequest::GlobalIndex { sender, global_index: gi_free_id };

        let new_gi_events = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncRequest::SyncRequest(sync_request))
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
                        .send_to_service(DataSyncRequest::Event(ss_event.clone()))
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
                key: KvKey::unit(ObjectDescriptor::DbTail),
                value: new_db_tail.clone(),
            },
        });

        self.persistent_object.repo.save(new_db_tail_event).await?;
        Ok(())
    }
}
