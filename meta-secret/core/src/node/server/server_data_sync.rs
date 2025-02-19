use std::cmp::PartialEq;
use std::sync::Arc;

use crate::node::common::model::device::common::{DeviceData, DeviceId};
use crate::node::common::model::secret::{SecretDistributionType, SsDistributionStatus};
use crate::node::common::model::vault::vault::VaultStatus;
use crate::node::db::actions::vault::vault_action::ServerVaultAction;
use crate::node::db::descriptors::shared_secret_descriptor::SsDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::shared_secret_event::{SsDistributionObject, SsLogObject};
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::{SsRequest, VaultRequest};
use anyhow::Result;
use anyhow::{anyhow, bail, Ok};
use async_trait::async_trait;
use derive_more::From;
use tracing::{info, instrument};

#[async_trait(? Send)]
pub trait DataSyncApi {
    async fn vault_replication(
        &self,
        vault_request: VaultRequest,
    ) -> Result<Vec<GenericKvLogEvent>>;

    async fn handle_write(&self, server_device: DeviceData, event: GenericKvLogEvent)
        -> Result<()>;
}

#[derive(From)]
pub struct ServerSyncGateway<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncResponse {
    Empty,
    Data(DataEventsResponse),
    ServerTailResponse(ServerTailResponse),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataEventsResponse(pub Vec<GenericKvLogEvent>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTailResponse {
    pub device_log_tail: Option<ArtifactId>,
    pub ss_device_log_tail: Option<ArtifactId>,
}

impl DataSyncResponse {
    pub fn to_data(&self) -> Result<DataEventsResponse> {
        match self {
            DataSyncResponse::Data(data) => Ok(data.clone()),
            _ => Err(anyhow!("Invalid response type")),
        }
    }

    pub fn to_server_tail(&self) -> Result<ServerTailResponse> {
        match self {
            DataSyncResponse::ServerTailResponse(server_tail) => Ok(server_tail.clone()),
            _ => Err(anyhow!("Invalid response type")),
        }
    }
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo> DataSyncApi for ServerSyncGateway<Repo> {
    #[instrument(skip(self))]
    async fn vault_replication(
        &self,
        vault_request: VaultRequest,
    ) -> Result<Vec<GenericKvLogEvent>> {
        let mut commit_log = vec![];
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };

        let vault_status = p_vault.find(vault_request.sender.clone()).await?;
        if let VaultStatus::Member { .. } = vault_status {
            let vault_events = self.vault_replication(vault_request.clone()).await?;
            commit_log.extend(vault_events);
        }

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled
    /// and the actions will be executed accordingly
    async fn handle_write(
        &self,
        server_device: DeviceData,
        generic_event: GenericKvLogEvent,
    ) -> Result<()> {
        self.server_write_processing(server_device, generic_event)
            .await
    }
}

impl<Repo: KvLogEventRepo> ServerSyncGateway<Repo> {
    #[instrument(skip(self))]
    async fn server_write_processing(
        &self,
        server_device: DeviceData,
        generic_event: GenericKvLogEvent,
    ) -> Result<()> {
        match generic_event {
            GenericKvLogEvent::DeviceLog(device_log_obj) => {
                self.handle_device_log_request(server_device, device_log_obj)
                    .await?;
            }
            GenericKvLogEvent::SsDeviceLog(ss_device_log_obj) => {
                info!(
                    "Shared Secret Device Log message processing: {:?}",
                    &ss_device_log_obj
                );

                self.p_obj.repo.save(ss_device_log_obj.clone()).await?;

                let ss_claim = ss_device_log_obj.get_distribution_request();
                let p_ss_log = PersistentSharedSecret::from(self.p_obj.clone());
                p_ss_log.save_ss_log_event(ss_claim).await?;
            }
            GenericKvLogEvent::SsDistribution(ss_object) => {
                self.p_obj.repo.save(ss_object).await?;
            }
            GenericKvLogEvent::Credentials(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::VaultLog(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::Vault(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::VaultStatus(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::SsLog(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::DbError(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn handle_device_log_request(
        &self,
        server_device: DeviceData,
        device_log_obj: DeviceLogObject,
    ) -> Result<()> {
        self.p_obj.repo.save(device_log_obj.clone()).await?;

        let vault_action_event = device_log_obj.0;
        let vault_action = vault_action_event.value;

        let action = ServerVaultAction {
            p_obj: self.p_obj.clone(),
            server_device,
        };

        action.do_processing(vault_action).await?;
        Ok(())
    }

    pub async fn vault_replication(&self, request: VaultRequest) -> Result<Vec<GenericKvLogEvent>> {
        let mut commit_log = vec![];

        let p_vault = PersistentVault::from(self.p_obj.clone());

        let vault_status = p_vault
            .update_vault_membership_info_for_user(request.sender.clone())
            .await?;

        //sync vault status (available to any user - just by definition)
        {
            let vault_status_events = self
                .p_obj
                .find_object_events::<GenericKvLogEvent>(request.tail.vault_status.clone())
                .await?;

            commit_log.extend(vault_status_events);
        }

        // guarding vault from sending event to outsiders
        match vault_status {
            VaultStatus::NotExists(_) => {
                //ignore
            }
            VaultStatus::Outsider(_) => {
                //ignore
            }
            VaultStatus::Member(_) => {
                //sync VaultLog
                {
                    let vault_log_events = self
                        .p_obj
                        .find_object_events::<GenericKvLogEvent>(request.tail.vault_log.clone())
                        .await?;
                    commit_log.extend(vault_log_events);
                }

                //sync Vault
                {
                    let vault_events = self
                        .p_obj
                        .find_object_events::<GenericKvLogEvent>(request.tail.vault.clone())
                        .await?;
                    commit_log.extend(vault_events);
                }
            }
        }

        Ok(commit_log)
    }

    pub async fn ss_replication(
        &self,
        request: SsRequest,
        server_device: DeviceId,
    ) -> Result<Vec<GenericKvLogEvent>> {
        //sync SsLog
        let ss_log_events = self
            .p_obj
            .find_object_events::<SsLogObject>(request.ss_log.clone())
            .await?;

        let maybe_latest_ss_log_state = ss_log_events.last();
        let Some(latest_ss_log_state) = maybe_latest_ss_log_state else {
            // Return empty result if there are no new events in the db
            // (no latest element, means - empty collection of events)
            return Ok(vec![]);
        };
        
        let mut commit_log = vec![];
        for ss_log_event in ss_log_events.clone() {
            commit_log.push(ss_log_event.to_generic())
        }

        let ss_log_data = latest_ss_log_state.as_data();
        let mut updated_ss_log_data = ss_log_data.clone();
        let mut updated_state = false;

        for (_, claim) in ss_log_data.claims.iter() {
            // Distribute shares
            for dist_id in claim.claim_db_ids() {
                if claim.sender.eq(&server_device) {
                    bail!("Invalid state. Server can't manage encrypted shares");
                };

                let receiver_device = request.sender.device.device_id.clone();

                // complete distribution action by sending the distribution event to the receiver
                if dist_id.distribution_id.receiver.eq(&receiver_device) {
                    let desc = SsDescriptor::Distribution(dist_id.distribution_id.clone());
                    let ss_obj = self.p_obj.find_tail_event(desc).await?;

                    if let Some(dist_event) = ss_obj {
                        let ss_dist_obj_id = dist_event.obj_id();
                        commit_log.push(dist_event.to_generic());
                        self.p_obj.repo.delete(ss_dist_obj_id).await;

                        let claim_id = dist_id.claim_id.id;
                        updated_ss_log_data =
                            updated_ss_log_data.complete_for_device(claim_id, receiver_device);
                        updated_state = true;
                    }
                }
            }
        }

        if updated_state {
            let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
            let new_ss_log_obj = p_ss
                .create_new_ss_log_claim(updated_ss_log_data, request.sender.vault_name.clone())
                .await?;
            self.p_obj.repo.save(new_ss_log_obj.to_generic()).await?;
        }

        Ok(commit_log)
    }
}
