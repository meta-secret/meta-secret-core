use crate::node::db::events::object_id::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DbTail {
    Basic {
        global_index_id: Option<ObjectId>
    },
    Full {
        global_index_id: Option<ObjectId>,
        vault_audit_id: ObjectId,
        s_s_audit: ObjectId,
    }
}
