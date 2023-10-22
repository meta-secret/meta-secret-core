use crate::node::db::events::common::{LogEventKeyBasedRecord, MemPoolObject, MetaPassObject, SharedSecretObject};
use crate::node::db::events::error::ErrorMessage;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::{DbTailObject, DeviceCredentialsObject};
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
    DeviceCredentials(DeviceCredentialsObject),
    DbTail(DbTailObject),

    Error { event: KvLogEvent<ErrorMessage> },
}

impl LogEventKeyBasedRecord for GenericKvLogEvent {
    fn key(&self) -> &KvKey {
        match self {
            GenericKvLogEvent::GlobalIndex(gi_obj) => gi_obj.key(),
            GenericKvLogEvent::Vault(vault_obj) => vault_obj.key(),
            GenericKvLogEvent::MetaPass(pass_obj) => pass_obj.key(),
            GenericKvLogEvent::SharedSecret(obj) => obj.key(),
            GenericKvLogEvent::MemPool(mem_pool_obj) => mem_pool_obj.key(),
            GenericKvLogEvent::Error { event } => &event.key,
            GenericKvLogEvent::DeviceCredentials(obj) => &obj.event.key,
            GenericKvLogEvent::DbTail(obj) => &obj.event.key
        }
    }
}

pub trait UnitEvent<T> {
    fn unit(value: T) -> Self;
}

pub trait UnitEventEmptyValue {
    fn unit() -> Self;
}
