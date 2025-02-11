use std::cmp::PartialEq;
use std::sync::Arc;

use crate::node::common::model::device::common::{DeviceData, DeviceId};
use crate::node::common::model::secret::{SecretDistributionType, SsDistributionStatus};
use crate::node::common::model::vault::vault::VaultStatus;
use crate::node::db::actions::vault::vault_action::ServerVaultAction;
use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::shared_secret_event::SharedSecretObject;
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::{SsRequest, VaultRequest};
use anyhow::Result;
use anyhow::{anyhow, bail, Ok};
use async_trait::async_trait;
use tracing::{info, instrument};

#[async_trait(? Send)]
pub trait DataSyncApi {
    async fn vault_replication(
        &self,
        vault_request: VaultRequest,
    ) -> Result<Vec<GenericKvLogEvent>>;

    async fn ss_replication(
        &self,
        ss_request: SsRequest,
        server_device: DeviceId,
    ) -> Result<Vec<GenericKvLogEvent>>;

    async fn handle(&self, server_device: DeviceData, event: GenericKvLogEvent) -> Result<()>;
}

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

    async fn ss_replication(
        &self,
        ss_request: SsRequest,
        server_device: DeviceId,
    ) -> Result<Vec<GenericKvLogEvent>> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };

        let vault_status = p_vault.find(ss_request.sender.clone()).await?;
        if let VaultStatus::Member { .. } = vault_status {
            let vault_events = self
                .ss_replication(ss_request.clone(), server_device)
                .await?;
            commit_log.extend(vault_events);
        }

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled
    /// and the actions will be executed accordingly
    async fn handle(
        &self,
        server_device: DeviceData,
        generic_event: GenericKvLogEvent,
    ) -> Result<()> {
        self.server_processing(server_device, generic_event).await
    }
}

impl<Repo: KvLogEventRepo> ServerSyncGateway<Repo> {
    #[instrument(skip(self))]
    async fn server_processing(
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

                self.p_obj
                    .repo
                    .save(ss_device_log_obj.clone().to_generic())
                    .await?;

                let ss_claim = ss_device_log_obj.get_distribution_request();

                let p_ss_log = PersistentSharedSecret {
                    p_obj: self.p_obj.clone(),
                };
                p_ss_log.save_ss_log_event(ss_claim).await?;
            }
            GenericKvLogEvent::SharedSecret(_) => {
                self.p_obj.repo.save(generic_event.clone()).await?;
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

        /// guarding vault from sending event to outsiders
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
        let mut commit_log = vec![];
        //sync SsLog
        let ss_log = self
            .p_obj
            .find_object_events::<GenericKvLogEvent>(request.ss_log.clone())
            .await?;
        commit_log.extend(ss_log.clone());

        let maybe_latest_ss_log_state = ss_log.last();
        let Some(latest_ss_log_state) = maybe_latest_ss_log_state else {
            return Ok(commit_log);
        };

        let ss_log_data = latest_ss_log_state.clone().ss_log()?.to_data();

        for (_, claim) in &ss_log_data.claims {
            for dist_id in claim.claim_db_ids() {
                let desc = SharedSecretDescriptor::SsDistribution(dist_id.distribution_id.clone());

                if claim.sender.eq(&server_device) {
                    bail!("Local shares must not be sent to server");
                };

                let sender_device = request.sender.device.device_id.clone();

                if sender_device.eq(&claim.sender) {
                    let ss_obj = self.p_obj.find_tail_event(desc).await?;

                    if let Some(dist_event) = ss_obj {
                        let p_ss = PersistentSharedSecret {
                            p_obj: self.p_obj.clone(),
                        };
                        p_ss.create_distribution_completion_status(dist_id).await?;

                        commit_log.push(dist_event.clone().to_generic());
                        self.p_obj.repo.delete(dist_event.obj_id()).await;
                    }
                }
            }

            let mut completed_split_claims = vec![];
            for (_, claim) in &ss_log_data.claims {
                if claim.distribution_type != SecretDistributionType::Split {
                    continue;
                }

                let mut completed = true;
                for dist_id in claim.claim_db_ids() {
                    let desc = SharedSecretDescriptor::SsDistributionStatus(dist_id.clone());
                    let maybe_status_event = self.p_obj.find_tail_event(desc).await?;

                    let Some(ss_obj) = maybe_status_event else {
                        completed = false;
                        break;
                    };

                    let SharedSecretObject::SsDistributionStatus(status_record) = ss_obj else {
                        completed = false;
                        break;
                    };

                    if status_record.value != SsDistributionStatus::Delivered {
                        completed = false;
                        break;
                    }
                }

                if completed {
                    completed_split_claims.push(claim.clone());
                }
            }

            let mut updated_ss_log_data = ss_log_data.clone();
            for completed_claim in &completed_split_claims {
                //complete distribution process by deleting the claim from ss_log
                updated_ss_log_data.claims.remove(&completed_claim.id);
            }

            if !completed_split_claims.is_empty() {
                let p_ss = PersistentSharedSecret {
                    p_obj: self.p_obj.clone(),
                };
                let new_ss_log_obj = p_ss
                    .create_new_ss_log_claim(updated_ss_log_data, request.sender.vault_name.clone())
                    .await?;
                self.p_obj.repo.save(new_ss_log_obj.to_generic()).await?;
            }
        }

        Ok(commit_log)
    }
}
