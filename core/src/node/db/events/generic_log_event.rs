use crate::node::db::events::common::{LogEventKeyBasedRecord, MemPoolObject, MetaPassObject, SharedSecretObject};
use crate::node::db::events::error::ErrorMessage;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::KvLogEventLocal;
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
    LocalEvent(KvLogEventLocal),

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
            GenericKvLogEvent::LocalEvent(op) => op.key(),
            GenericKvLogEvent::Error { event } => &event.key,
        }
    }
}
