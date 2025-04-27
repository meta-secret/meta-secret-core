use crate::node::common::model::secret::SsRecoveryId;
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::objects::persistent_vault::VaultTail;
use anyhow::{Result, anyhow};
use derive_more::From;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadSyncRequest {
    Vault(VaultRequest),
    SsRequest(SsRequest),
    SsRecoveryCompletion(SsRecoveryCompletion),
    ServerTail(ServerTailRequest),
}

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WriteSyncRequest {
    Event(GenericKvLogEvent),
}

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncRequest {
    Read(ReadSyncRequest),
    Write(WriteSyncRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsRecoveryCompletion {
    pub vault_name: VaultName,
    pub recovery_id: SsRecoveryId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultRequest {
    pub sender: UserData,
    pub tail: VaultTail,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsRequest {
    pub sender: UserData,
    pub ss_log: ArtifactId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTailRequest {
    pub sender: UserData,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_log_tail: Option<ArtifactId>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
