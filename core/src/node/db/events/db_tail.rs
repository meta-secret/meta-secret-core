use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTail {
    pub global_index_id: Option<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthDbTail {
    pub db_tail: DbTail,

    pub vault_audit_id: ObjectId,
    pub s_s_audit: ObjectId,
}

impl Default for DbTail {
    fn default() -> Self {
        Self { global_index_id: None }
    }
}