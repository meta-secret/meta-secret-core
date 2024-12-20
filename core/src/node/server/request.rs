use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::user::common::UserData;
use crate::node::db::events::object_id::ObjectId;
use serde::{Deserialize, Serialize};
use crate::node::db::objects::persistent_vault::VaultTail;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncRequest {
    GlobalIndex(GlobalIndexRequest),
    Vault(VaultRequest),
    Ss(SsRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultRequest {
    pub sender: UserData,
    pub tail: VaultTail,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalIndexRequest {
    pub sender: DeviceData,
    pub global_index: ObjectId,
}

impl GlobalIndexRequest {
    pub fn to_sync_request(self) -> SyncRequest {
        SyncRequest::GlobalIndex(self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsRequest {
    pub sender: UserData,
    pub ss_log: ObjectId,
}
