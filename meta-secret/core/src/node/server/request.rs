use crate::node::common::model::user::common::UserData;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::objects::persistent_vault::VaultTail;
use derive_more::From;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncRequest {
    Vault(VaultRequest),
    Ss(SsRequest),
    ServerTail(ServerTailRequest),
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
    pub ss_log: ObjectId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTailRequest {
    pub sender: UserData
}