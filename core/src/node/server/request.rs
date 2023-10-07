use serde::{Deserialize, Serialize};

use crate::models::UserSignature;
use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SyncRequest {
    pub sender: UserSignature,

    pub vault_tail_id: Option<ObjectId>,
    pub meta_pass_tail_id: Option<ObjectId>,

    pub global_index: Option<ObjectId>,
    
    pub s_s_audit: Option<ObjectId>
}
