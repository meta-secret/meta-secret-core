use crate::node::db::events::object_id::ObjectId;
use crate::node::db::models::PublicKeyRecord;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct SyncRequest {
    pub sender_pk: PublicKeyRecord,

    pub vault_tail_id: Option<ObjectId>,
    pub meta_pass_tail_id: Option<ObjectId>,

    pub global_index: Option<ObjectId>,
}
