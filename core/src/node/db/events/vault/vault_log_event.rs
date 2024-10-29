use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, ObjectId, VaultGenesisEvent, VaultUnitEvent};
use crate::node::db::events::vault_event::VaultActionEvent;
use anyhow::anyhow;

/// VaultLog keeps incoming events in order, the log is a queue for incoming messages and used to
/// recreate the vault state from events (event sourcing)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultLogObject {
    Unit(VaultUnitEvent),
    Genesis(VaultGenesisEvent),
    Action(KvLogEvent<ArtifactId, VaultActionEvent>),
}

impl VaultLogObject {
    pub fn unit(vault_name: VaultName) -> Self {
        let desc = VaultDescriptor::vault_log(vault_name.clone());

        VaultLogObject::Unit(VaultUnitEvent(KvLogEvent {
            key: KvKey::unit(desc),
            value: vault_name,
        }))
    }

    pub fn genesis(vault_name: VaultName, candidate: UserData) -> Self {
        let desc = VaultDescriptor::vault_log(vault_name.clone());
        VaultLogObject::Genesis(VaultGenesisEvent(KvLogEvent {
            key: KvKey::genesis(desc),
            value: candidate.clone(),
        }))
    }
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
