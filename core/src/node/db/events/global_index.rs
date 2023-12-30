use crate::node::common::model::device::DeviceData;
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::global_index::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent, UnitEventWithEmptyValue,
};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, GenesisId, ObjectId, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlobalIndexObject {
    Unit(KvLogEvent<UnitId, ()>),
    Genesis(KvLogEvent<GenesisId, DeviceData>),
    Update(KvLogEvent<ArtifactId, UnitId>),

    VaultIndex(KvLogEvent<UnitId, ()>),
}

impl GlobalIndexObject {
    pub fn index_from_vault_name(vault_name: VaultName) -> GlobalIndexObject {
        let vault_id = UnitId::vault_unit(vault_name);

        GlobalIndexObject::index_from_vault_id(vault_id)
    }

    pub fn index_from_vault_id(vault_id: UnitId) -> GlobalIndexObject {
        let idx_desc = ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::VaultIndex {
            vault_id: vault_id.clone(),
        });

        GlobalIndexObject::VaultIndex(KvLogEvent {
            key: KvKey::unit(idx_desc),
            value: (),
        })
    }
}

impl ToGenericEvent for GlobalIndexObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::GlobalIndex(self)
    }
}

impl ObjIdExtractor for GlobalIndexObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            GlobalIndexObject::Unit(event) => ObjectId::from(event.key.obj_id.clone()),
            GlobalIndexObject::Genesis(event) => ObjectId::from(event.key.obj_id.clone()),
            GlobalIndexObject::Update(event) => ObjectId::from(event.key.obj_id.clone()),
            GlobalIndexObject::VaultIndex(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl KeyExtractor for GlobalIndexObject {
    fn key(&self) -> GenericKvKey {
        match self {
            GlobalIndexObject::Unit(event) => GenericKvKey::from(event.key.clone()),
            GlobalIndexObject::Genesis(event) => GenericKvKey::from(event.key.clone()),
            GlobalIndexObject::Update(event) => GenericKvKey::from(event.key.clone()),
            GlobalIndexObject::VaultIndex(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl UnitEventWithEmptyValue for GlobalIndexObject {
    fn unit() -> Self {
        GlobalIndexObject::Unit(KvLogEvent::global_index_unit())
    }
}

impl GlobalIndexObject {
    pub fn genesis(server_pk: DeviceData) -> Self {
        GlobalIndexObject::Genesis(KvLogEvent::global_index_genesis(server_pk))
    }
}

impl TryFrom<GenericKvLogEvent> for GlobalIndexObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::GlobalIndex(gi_obj) = event {
            Ok(gi_obj)
        } else {
            Err(anyhow::anyhow!("Not a GlobalIndexObject"))
        }
    }
}
