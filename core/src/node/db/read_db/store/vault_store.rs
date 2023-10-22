use crate::models::VaultDoc;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::read_db::read_db_view::TailId;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum VaultStore {
    Empty,
    Unit {
        tail_id: ObjectId,
    },
    Genesis {
        tail_id: ObjectId,
        server_pk: PublicKeyRecord,
    },
    Store {
        tail_id: ObjectId,
        server_pk: PublicKeyRecord,
        vault: VaultDoc,
    },
}

impl TailId for VaultStore {
    fn tail_id(&self) -> Option<ObjectId> {
        match self {
            VaultStore::Empty => None,
            VaultStore::Unit { tail_id } => Some(tail_id.clone()),
            VaultStore::Genesis { tail_id, .. } => Some(tail_id.clone()),
            VaultStore::Store { tail_id, .. } => Some(tail_id.clone()),
        }
    }
}
