use serde::{Deserialize, Serialize};

use crate::node::common::model::device::DeviceData;
use crate::node::common::model::user::UserData;
use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SyncRequest {
    GlobalIndex(GlobalIndexRequest),
    Vault(VaultRequest),
    SharedSecret(SharedSecretRequest),
}

pub struct VaultRequest {
    pub sender: UserData,
    pub vault_log: ObjectId,
    pub vault: ObjectId,
    pub vault_status: ObjectId,
}

pub struct SharedSecretRequest {
    pub sender: UserData,
    pub ss_log: ObjectId,
}

pub struct GlobalIndexRequest {
    pub sender: DeviceData,
    pub global_index: ObjectId,
}