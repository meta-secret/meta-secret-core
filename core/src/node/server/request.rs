use serde::{Deserialize, Serialize};

use crate::node::common::model::device::DeviceData;
use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SyncRequest {
    GlobalIndex {
        sender: DeviceData,
        global_index: ObjectId,
    },
    Vault {
        request: VaultRequest
    },
    SharedSecret {
        sender: DeviceData,
        ss_log: ObjectId,
    },
}

pub struct VaultRequest {
    pub sender: DeviceData,
    pub vault_log: ObjectId,
    pub vault: ObjectId,
    pub vault_status: ObjectId,
}

