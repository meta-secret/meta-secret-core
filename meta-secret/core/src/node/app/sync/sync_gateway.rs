use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, error, info, instrument};

use crate::node::api::{
    DataEventsResponse, ReadSyncRequest, ServerTailRequest, ServerTailResponse, SsRequest,
    SyncRequest, VaultRequest, WriteSyncRequest,
};
use crate::node::app::sync::sync_protocol::SyncProtocol;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{SecretDistributionType, SsDistributionStatus};
use crate::node::common::model::user::common::{UserData, UserId};
use crate::node::common::model::vault::vault::VaultStatus;
use crate::node::db::descriptors::shared_secret_descriptor::{
    SsDeviceLogDescriptor, SsLogDescriptor,
};
use crate::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
use crate::node::db::events::generic_log_event::{ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::shared_secret_event::SsDeviceLogObject;
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use anyhow::Result;
use crate::crypto::keys::TransportSk;

pub struct SyncGateway<Repo: KvLogEventRepo, Sync: SyncProtocol> {
    pub id: String,
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub sync: Arc<Sync>,
    pub master_key: TransportSk,
}

impl<Repo: KvLogEventRepo, Sync: SyncProtocol> SyncGateway<Repo, Sync> {
    #[instrument(skip_all)]
    pub async fn run(&self) {
        info!("Run sync gateway");

        loop {
            let creds_repo = PersistentCredentials {
                p_obj: self.p_obj.clone(),
                master_key: self.master_key.clone()
            };

            let maybe_user_creds = creds_repo.get_user_creds().await.unwrap();
            let Some(user_creds) = maybe_user_creds else {
                async_std::task::sleep(Duration::from_millis(300)).await;
                continue;
            };

            let result = self.sync(user_creds.user()).await;
            if let Err(err) = result {
                error!("Sync error: {:?}", err);
            }

            async_std::task::sleep(Duration::from_millis(100)).await;
        }
    }

    ///Levels of synchronization:
    ///  - global index, server PK - when user has no account
    ///  - vault, shared secret... - user has been registered, we can sync vault related events
    #[instrument(skip_all)]
    pub async fn sync(&self, user: UserData) -> Result<()> {
        let server_tail = self.get_server_tail(user.clone()).await?;

        self.sync_device_log(&server_tail, user.user_id()).await?;

        let vault_sync_request = self.get_vault_request(user.clone()).await?;
        self.sync_vault(vault_sync_request).await?;

        self.sync_shared_secrets(&server_tail, user).await?;

        Ok(())
    }

    async fn get_server_tail(&self, user_data: UserData) -> Result<ServerTailResponse> {
        let server_tail = {
            let server_tail_sync_request = self.get_server_tail_request(user_data).await?;
            self.get_tail(server_tail_sync_request).await?
        };
        Ok(server_tail)
    }

    async fn get_tail(&self, server_tail_sync_request: SyncRequest) -> Result<ServerTailResponse> {
        let server_tail_response = self
            .sync
            .send(server_tail_sync_request)
            .await?
            .to_server_tail()?;

        Ok(server_tail_response)
    }

    async fn get_server_tail_request(&self, user_data: UserData) -> Result<SyncRequest> {
        let sync_request =
            SyncRequest::Read(Box::from(ReadSyncRequest::ServerTail(ServerTailRequest {
                sender: user_data,
            })));
        Ok(sync_request)
    }

    #[instrument(skip(self))]
    async fn sync_vault(&self, vault_sync_request: SyncRequest) -> Result<()> {
        let DataEventsResponse(data_sync_events) =
            self.sync.send(vault_sync_request).await?.to_data()?;

        for new_event in data_sync_events {
            debug!(
                "id: {:?}. Sync gateway. New event from server: {:?}",
                self.id, new_event
            );
            self.p_obj.repo.save(new_event).await?;
        }
        Ok(())
    }

    async fn get_vault_request(&self, user: UserData) -> Result<SyncRequest> {
        let vault_sync_request = {
            let sender = user.clone();
            let p_vault = PersistentVault::from(self.p_obj.clone());
            let tail = p_vault.vault_tail(user).await?;

            SyncRequest::Read(Box::from(ReadSyncRequest::Vault(VaultRequest {
                sender,
                tail,
            })))
        };
        Ok(vault_sync_request)
    }

    async fn sync_ss_device_log(
        &self,
        server_tail: &ServerTailResponse,
        device_id: DeviceId,
    ) -> Result<()> {
        let server_ss_device_log_tail_id = {
            let unit_id = || ArtifactId::from(SsDeviceLogDescriptor::from(device_id));
            server_tail
                .ss_device_log_tail
                .clone()
                .unwrap_or_else(unit_id)
        };

        let ss_device_log_events_to_sync: Vec<SsDeviceLogObject> = self
            .p_obj
            .find_object_events(server_ss_device_log_tail_id)
            .await?;

        for ss_device_log_event in ss_device_log_events_to_sync {
            let sync_request = SyncRequest::Write(Box::from(WriteSyncRequest::Event(
                ss_device_log_event.to_generic(),
            )));
            self.sync.send(sync_request).await?;
        }

        Ok(())
    }

    async fn sync_ss_log(&self, user: UserData) -> Result<()> {
        let vault_name = user.vault_name.clone();
        let ss_sync_request = {
            let ss_log_free_id = {
                let obj_desc = SsLogDescriptor::from(vault_name.clone());
                self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
            };

            SyncRequest::Read(Box::from(ReadSyncRequest::SsRequest(SsRequest {
                sender: user.clone(),
                ss_log: ss_log_free_id,
            })))
        };

        let DataEventsResponse(data_sync_events) =
            self.sync.send(ss_sync_request).await?.to_data()?;

        for new_event in data_sync_events {
            debug!(
                "id: {:?}. Sync gateway. New ss event from server: {:?}",
                self.id, new_event
            );
            self.p_obj.repo.save(new_event).await?;
        }

        // Send claims
        let maybe_ss_log = self
            .p_obj
            .find_tail_event(SsLogDescriptor::from(vault_name))
            .await?;

        if let Some(ss_log) = maybe_ss_log {
            for (_, claim) in ss_log.to_data().claims {
                let is_delivered = claim.status.status() == SsDistributionStatus::Delivered;
                if is_delivered {
                    continue;
                }

                let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
                match claim.distribution_type {
                    SecretDistributionType::Split => {
                        let wf_events = p_ss.get_distributions(claim.clone()).await?;

                        for wf_event in wf_events {
                            if claim.sender.eq(&user.device.device_id) {
                                let obj_id = wf_event.obj_id();
                                let request = {
                                    let event = WriteSyncRequest::Event(wf_event.to_generic());
                                    SyncRequest::Write(Box::from(event))
                                };
                                self.sync.send(request).await?;
                                self.p_obj.repo.delete(obj_id).await;
                            }
                        }
                    }
                    SecretDistributionType::Recover => {
                        if claim.sender.eq(&user.device.device_id) {
                            continue;
                        }

                        let wf_events = p_ss.get_recoveries(claim.clone()).await?;
                        for wf_event in wf_events {
                            let obj_id = wf_event.obj_id();
                            let request = {
                                let event = WriteSyncRequest::Event(wf_event.to_generic());
                                SyncRequest::Write(Box::from(event))
                            };
                            self.sync.send(request).await?;
                            self.p_obj.repo.delete(obj_id).await;
                        }
                    }
                };
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn sync_device_log(
        &self,
        server_tail: &ServerTailResponse,
        user_id: UserId,
    ) -> Result<()> {
        let device_log_events_to_sync = self.device_log_sync_request(server_tail, user_id).await?;
        for device_log_event in device_log_events_to_sync {
            self.sync.send(device_log_event).await?;
        }

        Ok(())
    }

    async fn device_log_sync_request(
        &self,
        server_tail: &ServerTailResponse,
        user_id: UserId,
    ) -> Result<Vec<SyncRequest>> {
        let tail_to_sync = match &server_tail.device_log_tail {
            None => ArtifactId::from(DeviceLogDescriptor::from(user_id)),
            Some(server_tail_id) => server_tail_id.clone(),
        };

        let device_log_events_to_sync: Vec<SyncRequest> = self
            .p_obj
            .find_object_events::<DeviceLogObject>(tail_to_sync)
            .await?
            .into_iter()
            .map(|device_log_event| {
                SyncRequest::Write(Box::from(WriteSyncRequest::Event(
                    device_log_event.to_generic(),
                )))
            })
            .collect();
        Ok(device_log_events_to_sync)
    }

    #[instrument(skip(self))]
    async fn sync_shared_secrets(
        &self,
        server_tail: &ServerTailResponse,
        user: UserData,
    ) -> Result<()> {
        let vault_status = {
            let p_vault = PersistentVault {
                p_obj: self.p_obj.clone(),
            };

            p_vault.find(user.clone()).await?
        };

        let VaultStatus::Member(_) = vault_status else {
            return Ok(());
        };

        //sync ss_device_log and ss_log
        self.sync_ss_device_log(server_tail, user.device.device_id.clone())
            .await?;
        self.sync_ss_log(user).await?;

        Ok(())
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::EmptyState;
    use crate::node::app::sync::sync_gateway::SyncGateway;
    use crate::node::app::sync::sync_protocol::SyncProtocol;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use std::sync::Arc;

    pub struct SyncGatewayFixture<Sync: SyncProtocol> {
        pub client_gw: Arc<SyncGateway<InMemKvLogEventRepo, Sync>>,
        pub vd_gw: Arc<SyncGateway<InMemKvLogEventRepo, Sync>>,
    }

    impl<Sync: SyncProtocol> SyncGatewayFixture<Sync> {
        pub fn from(state: &EmptyState, server_sync: Arc<Sync>) -> Self {
            let client_gw = Arc::new(SyncGateway {
                id: "client_gw".to_string(),
                p_obj: state.p_obj.client.clone(),
                sync: server_sync.clone(),
                master_key: state.device_creds.client_master_key.clone(),
            });

            let vd_gw = Arc::new(SyncGateway {
                id: "vd_gw".to_string(),
                p_obj: state.p_obj.vd.clone(),
                sync: server_sync,
                master_key: state.device_creds.vd_master_key.clone(),
            });

            Self { client_gw, vd_gw }
        }
    }
}
