use serde::{Deserialize, Serialize};

use crate::node::common::model::device::DeviceData;
use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SyncRequest {
    Basic {
        sender: DeviceData,
        request: BasicSyncRequest
    },
    Auth {
        sender: DeviceData,
        request: AuthSyncRequest
    }
}

pub struct BasicSyncRequest {
    global_index: ObjectId,
}

pub struct AuthSyncRequest {
    pub basic: BasicSyncRequest,
    pub vault_tail_id: ObjectId,
    pub meta_pass_tail_id: ObjectId,
    pub s_s_audit: ObjectId,
}
