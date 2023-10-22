use serde::{Deserialize, Serialize};

use crate::node::common::model::device::DeviceData;
use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SyncRequest {
    pub sender: DeviceData,

    pub vault_tail_id: Option<ObjectId>,
    pub meta_pass_tail_id: Option<ObjectId>,

    pub global_index: Option<ObjectId>,

    pub s_s_audit: Option<ObjectId>,
}
