use serde::{Deserialize, Serialize};

use crate::node::common::model::device::DeviceData;
use crate::node::common::model::user::UserData;
use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncRequest {
    GlobalIndex(GlobalIndexRequest),
    Vault(VaultRequest),
    SharedSecret(SharedSecretRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultRequest {
    pub sender: UserData,
    pub vault_log: ObjectId,
    pub vault: ObjectId,
    pub vault_status: ObjectId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSecretRequest {
    pub sender: UserData,
    pub ss_log: ObjectId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalIndexRequest {
    pub sender: DeviceData,
    pub global_index: ObjectId,
}