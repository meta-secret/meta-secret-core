use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{
    ArtifactId, GenesisId, Next, ObjectId, UnitId, VaultUnitEvent,
};
use anyhow::anyhow;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultObject {
    Unit(VaultUnitEvent),
    /// Vault creator
    Genesis(KvLogEvent<GenesisId, DeviceData>),
    Vault(KvLogEvent<ArtifactId, VaultData>),
}

impl VaultObject {
    pub fn sign_up(vault_name: VaultName, candidate: UserData) -> Self {
        let desc = VaultDescriptor::from(vault_name.clone());
        let vault_data = VaultData::from(candidate);

        let vault_id = UnitId::vault_unit(vault_name).next().next();

        let sign_up_event = KvLogEvent {
            key: KvKey::artifact(desc.to_obj_desc(), vault_id),
            value: vault_data,
        };
        VaultObject::Vault(sign_up_event)
    }
}

impl VaultObject {
    pub fn genesis(vault_name: VaultName, server_device: DeviceData) -> Self {
        let desc = VaultDescriptor::from(vault_name.clone());
        VaultObject::Genesis(KvLogEvent {
            key: KvKey::genesis(desc.to_obj_desc()),
            value: server_device,
        })
    }
}

impl VaultObject {
    pub fn unit(vault_name: VaultName) -> Self {
        let desc = VaultDescriptor::from(vault_name.clone());
        VaultObject::Unit(VaultUnitEvent(KvLogEvent {
            key: KvKey::unit(desc.to_obj_desc()),
            value: vault_name,
        }))
    }
}

impl TryFrom<GenericKvLogEvent> for VaultObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::Vault(vault) = event {
            Ok(vault)
        } else {
            Err(anyhow!("Not a vault event"))
        }
    }
}

impl ToGenericEvent for VaultObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::Vault(self)
    }
}

impl ObjIdExtractor for VaultObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            VaultObject::Genesis(event) => ObjectId::from(event.key.obj_id.clone()),
            VaultObject::Vault(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl KeyExtractor for VaultObject {
    fn key(&self) -> GenericKvKey {
        match self {
            VaultObject::Unit(event) => GenericKvKey::from(event.key()),
            VaultObject::Genesis(event) => GenericKvKey::from(event.key.clone()),
            VaultObject::Vault(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}
