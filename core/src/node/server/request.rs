use serde::{Deserialize, Serialize};

use crate::node::common::model::device::DeviceData;
use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SyncRequest {
    GlobalIndex {
        sender: DeviceData,
        request: GlobalIndexRequest
    },
    Vault {
        sender: DeviceData,
        request: VaultRequest
    }
}

pub struct GlobalIndexRequest {
    pub global_index: ObjectId,
}

pub struct VaultRequest {
    pub vault_tail_id: ObjectId,
    pub s_s_audit: ObjectId,
}
