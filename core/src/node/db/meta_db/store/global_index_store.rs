use std::collections::HashSet;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::meta_db::meta_db_view::TailId;

impl GlobalIndexStore {
    pub fn contains(&self, vault_id: String) -> bool {
        match self {
            GlobalIndexStore::Empty => false,
            GlobalIndexStore::Genesis { .. } => false,
            GlobalIndexStore::Store { global_index, .. } => global_index.contains(vault_id.as_str()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GlobalIndexStore {
    Empty,
    Genesis {
        tail_id: ObjectId,
        server_pk: PublicKeyRecord,
    },
    Store {
        server_pk: PublicKeyRecord,
        tail_id: ObjectId,
        global_index: HashSet<String>,
    },
}

impl TailId for GlobalIndexStore {
    fn tail_id(&self) -> Option<ObjectId> {
        match self {
            GlobalIndexStore::Empty => None,
            GlobalIndexStore::Genesis { tail_id, .. } => Some(tail_id.clone()),
            GlobalIndexStore::Store { tail_id, .. } => Some(tail_id.clone()),
        }
    }
}
