use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{ArtifactId, ObjectId, UnitId};

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTail {
    pub maybe_global_index_id: Option<ObjectId>,
    pub maybe_mem_pool_id: Option<ObjectId>,

    pub vault_id: ObjectIdDbEvent,
    pub meta_pass_id: ObjectIdDbEvent,
    pub s_s_audit: Option<ObjectId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Hash, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__db_tail_obj")]
pub enum ObjectIdDbEvent {
    Empty { obj_desc: ObjectDescriptor },
    Id { tail_id: ObjectId },
}
