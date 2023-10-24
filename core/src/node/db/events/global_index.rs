use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::generic_log_event::{KeyExtractor, ObjIdExtractor, UnitEventEmptyValue};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlobalIndexObject {
    Unit { event: KvLogEvent<UnitId, ()> },
    Genesis { event: KvLogEvent<GenesisId, PublicKeyRecord> },

    Update { event: KvLogEvent<ArtifactId, UnitId> },
    VaultIndex { event: KvLogEvent<UnitId, UnitId> },
}

impl ObjIdExtractor for GlobalIndexObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            GlobalIndexObject::Unit { event } => ObjectId::from(event.key.obj_id.clone()),
            GlobalIndexObject::Genesis { event } => ObjectId::from(event.key.obj_id.clone()),
            GlobalIndexObject::Update { event } => ObjectId::from(event.key.obj_id.clone()),
            GlobalIndexObject::VaultIndex { event } => ObjectId::from(event.key.obj_id.clone())
        }
    }
}

impl KeyExtractor for GlobalIndexObject {
    fn key(&self) -> GenericKvKey {
        match self {
            GlobalIndexObject::Unit { event } => GenericKvKey::from(event.key.clone()),
            GlobalIndexObject::Genesis { event } => GenericKvKey::from(event.key.clone()),
            GlobalIndexObject::Update { event } => GenericKvKey::from(event.key.clone()),
            GlobalIndexObject::VaultIndex { event } => GenericKvKey::from(event.key.clone())
        }
    }
}

impl UnitEventEmptyValue for GlobalIndexObject {
    fn unit() -> Self {
        GlobalIndexObject::Unit {
            event: KvLogEvent::global_index_unit(),
        }
    }
}

impl GlobalIndexObject {
    pub fn genesis(server_pk: &PublicKeyRecord) -> Self {
        GlobalIndexObject::Genesis {
            event: KvLogEvent::global_index_genesis(server_pk),
        }
    }
}
