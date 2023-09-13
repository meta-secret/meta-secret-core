use crate::models::MetaPasswordDoc;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::meta_db::meta_db_view::TailId;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MetaPassStore {
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
        passwords: Vec<MetaPasswordDoc>,
    },
}

impl MetaPassStore {
    pub fn passwords(&self) -> Vec<MetaPasswordDoc> {
        match self {
            MetaPassStore::Empty => {
                vec![]
            }
            MetaPassStore::Unit { .. } => {
                vec![]
            }
            MetaPassStore::Genesis { .. } => {
                vec![]
            }
            MetaPassStore::Store { passwords, .. } => passwords.clone(),
        }
    }
}

impl TailId for MetaPassStore {
    fn tail_id(&self) -> Option<ObjectId> {
        match self {
            MetaPassStore::Empty => None,
            MetaPassStore::Unit { tail_id } => Some(tail_id.clone()),
            MetaPassStore::Genesis { tail_id, .. } => Some(tail_id.clone()),
            MetaPassStore::Store { tail_id, .. } => Some(tail_id.clone()),
        }
    }
}
