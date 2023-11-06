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
        sender: DeviceData,
        request: VaultRequest
    }
}

pub struct VaultRequest {
    pub vault_audit: ObjectId,
    pub s_s_audit: ObjectId,
}

