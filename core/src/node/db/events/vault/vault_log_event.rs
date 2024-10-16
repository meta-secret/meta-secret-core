use anyhow::anyhow;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, ObjectId, VaultGenesisEvent, VaultUnitEvent};
use crate::node::db::events::vault_event::VaultAction;

/// VaultLog keeps incoming events in order, the log is a queue for incoming messages and used to
/// recreate the vault state from events (event sourcing)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultLogObject {
    Unit(VaultUnitEvent),
    Genesis(VaultGenesisEvent),
    Action(KvLogEvent<ArtifactId, VaultAction>),
}

impl TryFrom<GenericKvLogEvent> for VaultLogObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::VaultLog(vault_log) = event {
            Ok(vault_log)
        } else {
            Err(anyhow!("Not a vault log event"))
        }
    }
}

impl ToGenericEvent for VaultLogObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::VaultLog(self)
    }
}

impl KeyExtractor for VaultLogObject {
    fn key(&self) -> GenericKvKey {
        match self {
            VaultLogObject::Unit(event) => GenericKvKey::from(event.key().clone()),
            VaultLogObject::Genesis(event) => GenericKvKey::from(event.key().clone()),
            VaultLogObject::Action(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl ObjIdExtractor for VaultLogObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultLogObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            VaultLogObject::Genesis(event) => ObjectId::from(event.key().obj_id.clone()),
            VaultLogObject::Action(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}
