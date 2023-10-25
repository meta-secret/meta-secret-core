use crate::node::common::model::vault::VaultData;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};
use crate::node::db::read_db::read_db_view::TailId;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum VaultStore {
    Empty,

    Unit {
        id: UnitId,
    },

    Genesis {
        id: GenesisId,
        server_pk: PublicKeyRecord,
    },

    Store {
        tail_id: ArtifactId,
        server_pk: PublicKeyRecord,
        vault: VaultData,
    },
}

impl TailId for VaultStore {
    fn tail_id(&self) -> Option<ObjectId> {
        match self {
            VaultStore::Empty => None,
            VaultStore::Unit { id } => Some(ObjectId::from(id)),
            VaultStore::Genesis { id, .. } => Some(ObjectId::from(id)),
            VaultStore::Store { tail_id, .. } => Some(ObjectId::from(tail_id))
        }
    }
}
