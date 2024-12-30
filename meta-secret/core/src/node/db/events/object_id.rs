use serde_derive::{Deserialize, Serialize};

use crate::crypto::utils::NextId;
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{
    ObjectDescriptor, ObjectDescriptorId, ToObjectDescriptor,
};
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;

use super::kv_log_event::{KvKey, KvLogEvent};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectId {
    Unit(UnitId),
    Genesis(GenesisId),
    Artifact(ArtifactId),
}

impl ObjectId {
    pub fn id_str(&self) -> String {
        let id = match self {
            ObjectId::Unit(unit_id) => unit_id.id.clone(),
            ObjectId::Genesis(genesis_id) => genesis_id.id.clone(),
            ObjectId::Artifact(artifact_id) => artifact_id.id.clone(),
        };

        let id_str = format!("{}:{}::{}", id.fqdn.obj_type, id.fqdn.obj_instance, id.id);
        id_str
    }
}

pub trait Next<To> {
    fn next(self) -> To;
}

/// In category theory, a unit type is a fundamental concept that arises in the study of types and functions.
/// It is often denoted as the unit object, represented by the symbol "1" or "Unit."
/// The unit type serves as a foundational element within category theory,
/// providing a way to represent the absence of information or the presence of a single unique value.
///
/// Same here, Unit is a initial request to create/initialize an object, it's step zero.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitId {
    pub id: ObjectDescriptorId,
}

impl Next<GenesisId> for UnitId {
    fn next(self) -> GenesisId {
        GenesisId {
            id: self.id.clone().next_id(),
            unit_id: self,
        }
    }
}

impl From<ObjectDescriptorId> for UnitId {
    fn from(id: ObjectDescriptorId) -> Self {
        Self { id }
    }
}

impl From<UnitId> for ObjectId {
    fn from(value: UnitId) -> Self {
        ObjectId::Unit(value)
    }
}

impl From<GenesisId> for ObjectId {
    fn from(value: GenesisId) -> Self {
        ObjectId::Genesis(value)
    }
}

impl From<ArtifactId> for ObjectId {
    fn from(value: ArtifactId) -> Self {
        ObjectId::Artifact(value)
    }
}

impl ObjectId {
    pub fn unit(obj_desc: ObjectDescriptor) -> Self {
        ObjectId::Unit(UnitId::unit(&obj_desc))
    }

    pub fn genesis(obj_desc: ObjectDescriptor) -> Self {
        ObjectId::Genesis(GenesisId::genesis(obj_desc))
    }

    pub fn get_unit_id(&self) -> UnitId {
        match self {
            ObjectId::Unit(unit_id) => unit_id.clone(),
            ObjectId::Genesis(genesis_id) => genesis_id.unit_id.clone(),
            ObjectId::Artifact(artifact_id) => artifact_id.unit_id.clone(),
        }
    }
}

impl Next<ObjectId> for ObjectId {
    fn next(self) -> ObjectId {
        match self {
            ObjectId::Unit(unit_id) => ObjectId::from(unit_id.next()),
            ObjectId::Genesis(genesis_id) => ObjectId::from(genesis_id.next()),
            ObjectId::Artifact(artifact_id) => ObjectId::from(artifact_id.next()),
        }
    }
}

impl UnitId {
    pub fn unit(obj_descriptor: &ObjectDescriptor) -> UnitId {
        let fqdn = obj_descriptor.fqdn();
        UnitId { id: fqdn.next_id() }
    }

    pub fn db_tail() -> UnitId {
        UnitId::unit(&ObjectDescriptor::DbTail)
    }

    pub fn vault_unit(vault_name: VaultName) -> UnitId {
        let vault_desc = VaultDescriptor::Vault(vault_name.clone()).to_obj_desc();
        UnitId::unit(&vault_desc)
    }
}

/// Next step after Unit is Genesis, it's a first step in object initialization,
/// it contains digital signature and public key of the actor (for instance it could be meta secret server) that
/// is responsible to create a persistent object
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenesisId {
    id: ObjectDescriptorId,
    unit_id: UnitId,
}

impl Next<ArtifactId> for GenesisId {
    fn next(self) -> ArtifactId {
        ArtifactId {
            id: self.id.clone().next_id(),
            prev_id: self.id,
            unit_id: self.unit_id,
        }
    }
}

impl GenesisId {
    pub fn genesis(obj_desc: ObjectDescriptor) -> GenesisId {
        let unit_id = UnitId::unit(&obj_desc);
        unit_id.next()
    }
}

/// Any regular request or update event in the objects' lifetime
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactId {
    id: ObjectDescriptorId,
    prev_id: ObjectDescriptorId,
    unit_id: UnitId,
}

/// Generate next artifact from the previous one
impl Next<ArtifactId> for ArtifactId {
    fn next(self) -> ArtifactId {
        ArtifactId {
            id: self.id.clone().next_id(),
            prev_id: self.id,
            unit_id: self.unit_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultUnitEvent(pub KvLogEvent<UnitId, VaultName>);

impl VaultUnitEvent {
    pub fn key(&self) -> KvKey<UnitId> {
        self.0.key.clone()
    }

    pub fn vault_name(&self) -> VaultName {
        self.0.value.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultGenesisEvent(pub KvLogEvent<GenesisId, UserData>);

impl VaultGenesisEvent {
    pub fn key(&self) -> KvKey<GenesisId> {
        self.0.key.clone()
    }

    pub fn user(&self) -> UserData {
        self.0.value.clone()
    }
}
