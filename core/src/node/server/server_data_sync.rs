use std::cmp::PartialEq;
use std::sync::Arc;

use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::device::device_link::DeviceLink;
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::VaultStatus;
use crate::node::db::actions::vault::vault_action::VaultAction;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::events::shared_secret_event::{SsDeviceLogObject, SsLogObject};
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::{SsRequest, SyncRequest, VaultRequest};
use anyhow::Result;
use anyhow::{anyhow, bail, Ok};
use async_trait::async_trait;
use tracing::{info, instrument};

#[async_trait(? Send)]
pub trait DataSyncApi {
    async fn replication(&self, request: SyncRequest) -> anyhow::Result<Vec<GenericKvLogEvent>>;
    async fn handle(&self, server_device: DeviceData, event: GenericKvLogEvent) -> anyhow::Result<()>;
}

pub struct ServerSyncGateway<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncRequest {
    SyncRequest(SyncRequest),
    ServerTailRequest(UserData),
    Event(GenericKvLogEvent),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncResponse {
    Data(DataEventsResponse),
    ServerTailResponse(ServerTailResponse),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataEventsResponse(pub Vec<GenericKvLogEvent>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTailResponse {
    pub device_log_tail: Option<ObjectId>,
    pub ss_device_log_tail: Option<ObjectId>,
}

impl DataSyncResponse {
    pub fn to_data(&self) -> Result<DataEventsResponse> {
        match self {
            DataSyncResponse::Data(data) => Ok(data.clone()),
            _ => Err(anyhow!("Invalid response type")),
        }
    }

    pub fn to_server_tail(&self) -> anyhow::Result<ServerTailResponse> {
        match self {
            DataSyncResponse::ServerTailResponse(server_tail) => Ok(server_tail.clone()),
            _ => Err(anyhow!("Invalid response type")),
        }
    }
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo> DataSyncApi for ServerSyncGateway<Repo> {
    #[instrument(skip(self))]
    async fn replication(&self, request: SyncRequest) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        match &request {
            SyncRequest::GlobalIndex(gi_request) => {
                let gi_events = self.global_index_replication(gi_request.global_index.clone()).await?;
                commit_log.extend(gi_events);
            }

            SyncRequest::Vault(vault_request) => {
                let p_vault = PersistentVault {
                    p_obj: self.p_obj.clone(),
                };

                let vault_status = p_vault.find(vault_request.sender.clone()).await?;
                match vault_status {
                    VaultStatus::NotExists(_) => {
                        return Ok(commit_log);
                    }
                    VaultStatus::Outsider(_) => {
                        return Ok(commit_log);
                    }
                    VaultStatus::Member { .. } => {
                        let vault_events = self.vault_replication(vault_request.clone()).await?;
                        commit_log.extend(vault_events);
                    }
                }
            }

            SyncRequest::Ss(ss_request) => {
                let p_vault = PersistentVault {
                    p_obj: self.p_obj.clone(),
                };

                let vault_status = p_vault.find(ss_request.sender.clone()).await?;
                match vault_status {
                    VaultStatus::NotExists(_) => {
                        return Ok(commit_log);
                    }
                    VaultStatus::Outsider(_) => {
                        return Ok(commit_log);
                    }
                    VaultStatus::Member { .. } => {
                        let vault_events = self.ss_replication(ss_request.clone()).await?;
                        commit_log.extend(vault_events);
                    }
                }
            }
        }

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled and the actions will be executed accordingly
    async fn handle(&self, server_device: DeviceData, generic_event: GenericKvLogEvent) -> anyhow::Result<()> {
        self.server_processing(server_device, generic_event).await
    }
}

impl<Repo: KvLogEventRepo> ServerSyncGateway<Repo> {
    #[instrument(skip(self))]
    async fn server_processing(&self, server_device: DeviceData, generic_event: GenericKvLogEvent) -> Result<()> {
        match &generic_event {
            GenericKvLogEvent::DeviceLog(device_log_obj) => {
                self.handle_device_log_request(server_device, device_log_obj).await?;
            }
            GenericKvLogEvent::SsDeviceLog(ss_device_log_obj) => {
                info!("Shared Secret Device Log message processing: {:?}", &ss_device_log_obj);

                self.p_obj.repo.save(ss_device_log_obj.clone().to_generic()).await?;

                if let SsDeviceLogObject::Claim(event) = ss_device_log_obj {
                    let ss_claim = event.value.clone();

                    let p_ss_log = PersistentSharedSecret {
                        p_obj: self.p_obj.clone(),
                    };
                    p_ss_log.save_ss_log_event(ss_claim).await?;
                }
            }
            GenericKvLogEvent::SharedSecret(_) => {
                self.p_obj.repo.save(generic_event.clone()).await?;
            }
            _ => {
                bail!("Invalid event type: {:?}", generic_event);
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn handle_device_log_request(
        &self,
        server_device: DeviceData,
        device_log_obj: &DeviceLogObject,
    ) -> Result<()> {
        self.p_obj.repo.save(device_log_obj.clone().to_generic()).await?;

        let vault_action_event = match device_log_obj {
            DeviceLogObject::Unit { .. } => {
                return Ok(());
            }
            DeviceLogObject::Genesis(_) => {
                return Ok(());
            }
            DeviceLogObject::Action(event) => event,
        };

        let vault_action = vault_action_event.value.clone();

        let action = VaultAction {
            p_obj: self.p_obj.clone(),
            server_device,
        };
        action.do_processing(vault_action).await?;
        Ok(())
    }

    #[instrument(skip_all)]
    async fn global_index_replication(&self, gi_id: ObjectId) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let events = self.p_obj.find_object_events(gi_id).await?;
        Ok(events)
    }

    pub async fn vault_replication(&self, request: VaultRequest) -> Result<Vec<GenericKvLogEvent>> {
        let mut commit_log = vec![];

        //sync VaultLog
        {
            let vault_log_events = self.p_obj.find_object_events(request.vault_log.clone()).await?;
            commit_log.extend(vault_log_events);
        }

        //sync Vault
        {
            let vault_events = self.p_obj.find_object_events(request.vault.clone()).await?;
            commit_log.extend(vault_events);
        }

        //sync vault status
        {
            let vault_status_events = self.p_obj.find_object_events(request.vault_status.clone()).await?;

            commit_log.extend(vault_status_events);
        }

        Ok(commit_log)
    }

    pub async fn ss_replication(&self, request: SsRequest) -> Result<Vec<GenericKvLogEvent>> {
        let mut commit_log = vec![];
        //sync SsLog
        let ss_log = self.p_obj.find_object_events(request.ss_log.clone()).await?;
        commit_log.extend(ss_log);

        let p_ss = PersistentSharedSecret {
            p_obj: self.p_obj.clone(),
        };
        let ss_log_obj = p_ss.get_ss_log_obj(request.sender.vault_name.clone()).await?;
        if let SsLogObject::Claims(claim_kv_event) = ss_log_obj {
            let ss_log_data = claim_kv_event.value;
            for (_, claim) in ss_log_data.claims {
                for dist_id in claim.distribution_ids() {
                    let desc = SharedSecretDescriptor::SsDistribution(dist_id.clone()).to_obj_desc();

                    match dist_id.device_link {
                        DeviceLink::Loopback(_) => {
                            bail!("Local shares must not be sent to server");
                        }
                        DeviceLink::PeerToPeer(p2p_device_link) => {
                            if request.sender.device.device_id.eq(p2p_device_link.receiver()) {
                                if let Some(dist_event) = self.p_obj.find_tail_event(desc).await? {
                                    commit_log.push(dist_event.clone());

                                    self.p_obj.repo.delete(dist_event.obj_id()).await;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(commit_log)
    }
}
