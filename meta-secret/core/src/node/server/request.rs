use crate::node::common::model::user::common::UserData;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::objects::persistent_vault::VaultTail;
use derive_more::From;
use serde::{Deserialize, Serialize};
use crate::node::common::model::secret::{SsRecoveryId};
use crate::node::common::model::vault::vault::VaultName;

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
