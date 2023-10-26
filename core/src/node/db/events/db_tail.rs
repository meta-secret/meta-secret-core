use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTail {
    pub global_index_id: Option<ObjectId>,
    pub mem_pool_id: Option<ObjectId>,
    pub auth: Option<AuthDbTail>
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthDbTail {
    pub vault_id: ObjectId,
    pub meta_pass_id: ObjectId,
    pub s_s_audit: ObjectId,
}

impl Default for DbTail {
    fn default() -> Self {
        Self {
            global_index_id: None,
            mem_pool_id: None,
            auth: None,
        }
    }
}