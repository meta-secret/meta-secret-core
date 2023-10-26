use crate::node::db::events::common::{MemPoolObject, MetaPassObject, SharedSecretObject};
use crate::node::db::events::error::ErrorMessage;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{GenericKvKey, KvLogEvent};
use crate::node::db::events::local::{DbTailObject, DeviceCredentialsObject};
use crate::node::db::events::object_id::{ArtifactId, ObjectId};
use crate::node::db::events::vault_event::VaultObject;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__generic_event_type")]
pub enum GenericKvLogEvent {
    GlobalIndex(GlobalIndexObject),
    Vault(VaultObject),
    MetaPass(MetaPassObject),
    SharedSecret(SharedSecretObject),
    MemPool(MemPoolObject),

    /// Local events (persistent objects which lives only in the local environment) which must not be synchronized
    Credentials(DeviceCredentialsObject),
    DbTail(DbTailObject),

    Error { event: KvLogEvent<ArtifactId, ErrorMessage> },
}

pub trait ObjIdExtractor {
    fn obj_id(&self) -> ObjectId;
}

pub trait KeyExtractor {
    fn key(&self) -> GenericKvKey;
}

impl ObjIdExtractor for GenericKvLogEvent {
    fn obj_id(&self) -> ObjectId {
        match self {
            GenericKvLogEvent::GlobalIndex(obj) => obj.obj_id(),
            GenericKvLogEvent::Vault(obj) => obj.obj_id(),
            GenericKvLogEvent::MetaPass(obj) => obj.obj_id(),
            GenericKvLogEvent::SharedSecret(obj) => obj.obj_id(),
            GenericKvLogEvent::MemPool(obj) => obj.obj_id(),
            GenericKvLogEvent::Credentials(obj) => obj.obj_id(),
            GenericKvLogEvent::DbTail(obj) => obj.obj_id(),
            GenericKvLogEvent::Error { event } => event.key.obj_id.clone(),
        }
    }
}

pub trait UnitEvent<T> {
    fn unit(value: T) -> Self;
}

pub trait UnitEventEmptyValue {
    fn unit() -> Self;
}
