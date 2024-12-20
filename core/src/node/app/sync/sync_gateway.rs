use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, error, info, instrument};

use crate::node::app::sync::global_index::GlobalIndexDbSync;
use crate::node::common::model::device::common::{DeviceData, DeviceId};
use crate::node::common::model::user::common::UserId;
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::VaultStatus;
use crate::node::db::descriptors::global_index_descriptor::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ToObjectDescriptor};
use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::generic_log_event::{ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local_event::{CredentialsObject, DbTailObject};
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::events::shared_secret_event::SsDeviceLogObject;
use crate::node::db::objects::global_index::ClientPersistentGlobalIndex;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::node::server::request::{GlobalIndexRequest, SsRequest, SyncRequest, VaultRequest};
use crate::node::server::server_app::ServerDataTransfer;
use crate::node::server::server_data_sync::{
    DataEventsResponse, DataSyncRequest, ServerTailResponse,
};
use anyhow::Result;


pub struct SyncGateway<Repo: KvLogEventRepo> {
    pub id: String,
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub server_dt: Arc<ServerDataTransfer>,
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

            async_std::task::sleep(Duration::from_secs(1)).await;
        }
    }

    ///Levels of synchronization:
    ///  - global index, server PK - when user has no account
    ///  - vault, shared secret... - user has been registered, we can sync vault related events
    #[instrument(skip_all)]
    pub async fn sync(&self) -> Result<()> {
        let creds_repo = PersistentCredentials {
            p_obj: self.p_obj.clone(),
        };

        let maybe_creds_event = creds_repo.find().await?;

        let Some(creds_obj) = maybe_creds_event else {
            error!("No device creds found on this device, skip");
            return Ok(());
        };

        self.download_global_index(creds_obj.device()).await?;

        let CredentialsObject::DefaultUser(user_creds_event) = creds_obj else {
            //info!("No user defined");
            return Ok(());
        };

        let user_creds = user_creds_event.value;

        self.sync_vault(&user_creds).await?;

        let server_tail_response = self.get_tail(&user_creds).await?;

        self.sync_device_log(&server_tail_response, user_creds.user_id())
            .await?;

        self.sync_shared_secrets(&server_tail_response, &user_creds)
            .await?;

        Ok(())
    }

    async fn get_tail(&self, user_creds: &UserCredentials) -> Result<ServerTailResponse> {
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };
        let vault_status = p_vault.find(user_creds.user()).await?;

        let server_tail_response = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncRequest::ServerTailRequest(vault_status.user()))
            .await?
            .to_server_tail()?;
        Ok(server_tail_response)
    }

    #[instrument(skip_all)]
    async fn sync_vault(&self, user_creds: &UserCredentials) -> Result<()> {
        let vault_sync_request = {
            let sender = user_creds.user();

            let p_vault = PersistentVault {
                p_obj: self.p_obj.clone(),
            };

            let tail = p_vault.vault_tail(user_creds.user()).await?;

            SyncRequest::Vault(VaultRequest { sender, tail })
        };

        let DataEventsResponse(data_sync_events) = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncRequest::SyncRequest(vault_sync_request))
            .await?
            .to_data()?;

        for new_event in data_sync_events {
            debug!(
                "id: {:?}. Sync gateway. New event from server: {:?}",
                self.id, new_event
            );
            self.p_obj.repo.save(new_event).await?;
        }
        Ok(())
    }

    async fn sync_ss_device_log(
        &self,
        server_tail: &ServerTailResponse,
        device_id: DeviceId,
    ) -> Result<()> {
        let server_ss_device_log_tail_id = {
            let unit_id = || {
                let desc = SharedSecretDescriptor::SsDeviceLog(device_id).to_obj_desc();
                ObjectId::unit(desc)
            };
            server_tail
                .ss_device_log_tail
                .clone()
                .unwrap_or_else(unit_id)
        };

        let ss_device_log_events_to_sync = self
            .p_obj
            .find_object_events(server_ss_device_log_tail_id)
            .await?;

        for ss_device_log_event in ss_device_log_events_to_sync {
            //get SsDistribution events
            let ss_device_log = ss_device_log_event.clone().ss_device_log()?;
            if let SsDeviceLogObject::Claim(claim_event) = ss_device_log {
                let distribution_claim = claim_event.value;
                let p_ss = PersistentSharedSecret {
                    p_obj: self.p_obj.clone(),
                };
                let dist_events = p_ss.get_ss_distribution_events(distribution_claim).await?;
                for dist_event in dist_events {
                    self.server_dt
                        .dt
                        .send_to_service(DataSyncRequest::Event(dist_event.clone()))
                        .await;

                    self.p_obj.repo.delete(dist_event.obj_id()).await;
                }
            }

            self.server_dt
                .dt
                .send_to_service(DataSyncRequest::Event(ss_device_log_event))
                .await;
        }

        Ok(())
    }

    async fn sync_ss_log(&self, user_creds: &UserCredentials) -> Result<()> {
        let vault_name = user_creds.vault_name.clone();
        let ss_sync_request = {
            let ss_log_free_id = {
                let obj_desc = SharedSecretDescriptor::SsLog(vault_name.clone()).to_obj_desc();
                self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
            };

            SyncRequest::Ss(SsRequest {
                sender: user_creds.user(),
                ss_log: ss_log_free_id,
            })
        };

        let DataEventsResponse(data_sync_events) = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncRequest::SyncRequest(ss_sync_request))
            .await?
            .to_data()?;

        for new_event in data_sync_events {
            debug!(
                "id: {:?}. Sync gateway. New ss event from server: {:?}",
                self.id, new_event
            );
            self.p_obj.repo.save(new_event).await?;
        }
        Ok(())
    }

    #[instrument(skip(self))]
    async fn sync_device_log(
        &self,
        server_tail: &ServerTailResponse,
        user_id: UserId,
    ) -> Result<()> {
        let tail_to_sync = match &server_tail.device_log_tail {
            None => ObjectId::unit(VaultDescriptor::device_log(user_id)),
            Some(server_tail_id) => server_tail_id.clone(),
        };

        let device_log_events_to_sync = self.p_obj.find_object_events(tail_to_sync).await?;

        for device_log_event in device_log_events_to_sync {
            self.server_dt
                .dt
                .send_to_service(DataSyncRequest::Event(device_log_event))
                .await;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn download_global_index(&self, sender: DeviceData) -> Result<()> {
        let gi_sync = GlobalIndexDbSync::new(self.p_obj.clone(), sender.clone());

        let sync_request = gi_sync.get_gi_request().await?.to_sync_request();

        let DataEventsResponse(new_gi_events) = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncRequest::SyncRequest(sync_request))
            .await?
            .to_data()?;

        gi_sync.save(new_gi_events).await
    }

    #[instrument(skip(self))]
    async fn sync_shared_secrets(
        &self,
        server_tail: &ServerTailResponse,
        creds: &UserCredentials,
    ) -> Result<()> {
        let vault_status = {
            let p_vault = PersistentVault {
                p_obj: self.p_obj.clone(),
            };

            p_vault.find(creds.user()).await?
        };

        let VaultStatus::Member { .. } = vault_status else {
            return Ok(());
        };

        //sync ss_device_log and ss_log
        self.sync_ss_device_log(&server_tail, creds.device().device_id)
            .await?;
        self.sync_ss_log(creds).await?;

        Ok(())
    }

    #[instrument(skip_all)]
    async fn save_updated_db_tail(&self, db_tail: DbTail, new_db_tail: DbTail) -> Result<()> {
        if new_db_tail == db_tail {
            return Ok(());
        }

        //update db_tail
        let new_db_tail_event = DbTailObject(KvLogEvent {
            key: KvKey::unit(ObjectDescriptor::DbTail),
            value: new_db_tail.clone(),
        })
        .to_generic();

        self.p_obj.repo.save(new_db_tail_event).await?;
        Ok(())
    }
}

#[cfg(test)]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::BaseState;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::app::sync::sync_gateway::SyncGateway;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use std::sync::Arc;

    pub struct SyncGatewayFixture {
        pub client_gw: Arc<SyncGateway<InMemKvLogEventRepo>>,
        pub vd_gw: Arc<SyncGateway<InMemKvLogEventRepo>>,
    }

    impl SyncGatewayFixture {
        pub fn from(registry: &FixtureRegistry<BaseState>) -> Self {
            let client_gw = Arc::new(SyncGateway {
                id: "client_gw".to_string(),
                p_obj: registry.state.empty.p_obj.client.clone(),
                server_dt: registry.state.server_dt.server_dt.clone(),
            });

            let vd_gw = Arc::new(SyncGateway {
                id: "vd_gw".to_string(),
                p_obj: registry.state.empty.p_obj.vd.clone(),
                server_dt: registry.state.server_dt.server_dt.clone(),
            });

            Self { client_gw, vd_gw }
        }
    }
}
