use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTail {
    pub maybe_global_index_id: Option<ObjectId>,
    pub maybe_mem_pool_id: Option<ObjectId>,

    pub vault_id: DbTailObject,
    pub meta_pass_id: DbTailObject,
    pub s_s_audit: Option<ObjectId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Hash, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__db_tail_obj")]
pub enum DbTailObject {
    Empty { unit_id: ObjectId },
    Id { tail_id: ObjectId },
}
