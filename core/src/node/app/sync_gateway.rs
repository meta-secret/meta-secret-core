use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use tracing::{debug, error, info, instrument};
use crate::node::common::model::device::DeviceData;

use crate::node::common::model::user::{UserCredentials, UserDataMember, UserMembership};
use crate::node::db::descriptors::global_index::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::descriptors::shared_secret::{SharedSecretDescriptor, SharedSecretEventId};
use crate::node::db::descriptors::vault::VaultDescriptor;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ToGenericEvent};
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::{CredentialsObject, DbTailObject};
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::data_sync::DataSyncRequest;
use crate::node::server::request::{GlobalIndexRequest, SharedSecretRequest, SyncRequest, VaultRequest};
use crate::node::server::server_app::ServerDataTransfer;

pub struct SyncGateway<Repo: KvLogEventRepo> {
    pub id: String,
    pub persistent_object: Arc<PersistentObject<Repo>>,
    pub server_dt: Arc<ServerDataTransfer>
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

        let maybe_creds_event = self.persistent_object
            .find_tail_event(ObjectDescriptor::CredsIndex)
            .await?;

        let Some(creds_event) = maybe_creds_event else {
            return Ok(());
        };

        let GenericKvLogEvent::Credentials(creds_obj) = creds_event else {
            return Ok(());
        };

        self.sync_global_index(creds_obj.device()).await?;

        let CredentialsObject::DefaultUser { event: user_creds_event } = creds_obj else {
            return Ok(());
        };

        //Vault synchronization
        let user_creds = user_creds_event.value;
        let sender = user_creds.user();

        let vault_status_obj_desc = ObjectDescriptor::Vault(VaultDescriptor::VaultStatus {
            device_id: user_creds.device().id,
            vault_name: user_creds.vault_name.clone(),
        });

        let maybe_vault_status = self
            .persistent_object
            .find_tail_event(vault_status_obj_desc)
            .await?;

        let Some(GenericKvLogEvent::VaultStatus(vault_status_object)) = maybe_vault_status else {
            return Err(anyhow!("Invalid event type: not a vault status"));
        };

        if vault_status_object.is_not_member() {
            return Ok(());
        };

        // Local device has less events than the server
        let sync_request = {
            let vault_name = user_creds.vault_name.clone();

            let vault_log_free_id = {
                let obj_desc = VaultDescriptor::vault_log(vault_name.clone());
                self.persistent_object.find_free_id_by_obj_desc(obj_desc).await?
            };

            let vault_free_id = {
                let obj_desc = VaultDescriptor::vault(vault_name.clone());
                self.persistent_object.find_free_id_by_obj_desc(obj_desc).await?
            };

            let vault_status_free_id = {
                let device_id = user_creds.device().id;
                let obj_desc = VaultDescriptor::vault_status(device_id, vault_name);
                self.persistent_object.find_free_id_by_obj_desc(obj_desc).await?
            };

            SyncRequest::Vault(VaultRequest {
                sender,
                vault_log: vault_log_free_id,
                vault: vault_free_id,
                vault_status: vault_status_free_id,
            })
        };

        let data_sync_response = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncRequest::SyncRequest(sync_request))
            .await?;

        for new_event in data_sync_response.events {
            debug!("id: {:?}. Sync gateway. New event: {:?}", self.id, new_event);
            self.persistent_object.repo.save(new_event).await?;
        }

        //TODO how a device should handle new events in the vault_log to update the vault and the vault_status tables?

        self.sync_shared_secrets(&user_creds).await;

        Ok(())
    }

    async fn sync_global_index(&self, sender: DeviceData) -> anyhow::Result<()> {
        //TODO optimization: read global index tail id from db_tail

        let gi_free_id = {
            let gi_desc = ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index);
            self.persistent_object
                .find_free_id_by_obj_desc(gi_desc)
                .await?
        };

        let sync_request = SyncRequest::GlobalIndex(GlobalIndexRequest {
            sender,
            global_index: gi_free_id,
        });

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
                    let vault_idx_evt = GlobalIndexObject::index_from_vault_id(vault_id).to_generic();
                    self.persistent_object.repo.save(vault_idx_evt).await?;
                }
            } else {
                Err(anyhow!("Invalid event: {:?}", gi_event.key().obj_desc()))
            }
        }
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn sync_shared_secrets(&self, creds: &UserCredentials) {
        let maybe_vault_event = {
            let obj_desc = VaultDescriptor::vault(creds.vault_name.clone());
            self.persistent_object.find_tail_event(obj_desc).await?
        };

        let Some(GenericKvLogEvent::Vault(VaultObject::Vault { event: vault_event })) = maybe_vault_event else {
            return;
        };

        let vault = vault_event.value;
        for (device_id, user) in vault.users {
            let UserMembership::Member(UserDataMember { user_data: member }) = user else {
                continue;
            };

            let ss_event_id = SharedSecretEventId {
                vault_name: creds.vault_name.clone(),
                sender: creds.device_creds.device.id.clone(),
                receiver: member.device.id.clone(),
            };

            let split_events = {
                let split_obj_desc = ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Split(ss_event_id.clone()));
                self.persistent_object
                    .get_object_events_from_beginning(split_obj_desc)
                    .await?
            };

            for split_event in split_events {
                self
                    .server_dt
                    .dt
                    .send_to_service(DataSyncRequest::Event(split_event))
                    .await?;
            }

            let recover_events = {
                let recover_obj_desc = ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Recover(ss_event_id));
                self.persistent_object
                    .get_object_events_from_beginning(recover_obj_desc)
                    .await?
            };
            for recover_event in recover_events {
                self.server_dt.dt
                    .send_to_service(DataSyncRequest::Event(recover_event))
                    .await?;
            }
        }

        let ss_sync_request = {
            let ss_log_obj_desc = ObjectDescriptor::SharedSecret(SharedSecretDescriptor::SSLog {
                vault_name: creds.vault_name.clone()
            });

            let ss_log_id = self.persistent_object
                .find_free_id_by_obj_desc(ss_log_obj_desc)
                .await?;

            SyncRequest::SharedSecret(SharedSecretRequest {
                sender: creds.user(),
                ss_log: ss_log_id
            })
        };

        let new_ss_log_events = self.server_dt.dt
            .send_to_service_and_get(DataSyncRequest::SyncRequest(ss_sync_request))
            .await?;

        for new_ss_log_event in new_ss_log_events.events {
            self.persistent_object.repo.save(new_ss_log_event).await?;
        }
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
