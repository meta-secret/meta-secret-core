use serde_derive::{Deserialize, Serialize};

use crate::crypto::utils::NextId;
use crate::node::db::events::object_descriptor::{ObjectDescriptor, ObjectDescriptorId};
use crate::node::db::events::object_descriptor::global_index::GlobalIndexDescriptor;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__obj_id")]
pub enum ObjectId {
    Unit(UnitId),
    Genesis(GenesisId),
    Artifact(ArtifactId),
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
            id: self.id.next_id(),
            unit_id: self,
        }
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
    pub fn unit(obj_desc: &ObjectDescriptor) -> Self {
        ObjectId::Unit(UnitId::unit(obj_desc)) 
    }
}

impl Next<ObjectId> for ObjectId {
    fn next(self) -> ObjectId {
        match self {
            ObjectId::Unit(unit_id) => ObjectId::from(unit_id.next()),
            ObjectId::Genesis(genesis_id) => ObjectId::from(genesis_id.next()),
            ObjectId::Artifact(artifact_id) => ObjectId::from(artifact_id.next())
        }
    }
}

impl UnitId {
    pub fn unit(obj_descriptor: &ObjectDescriptor) -> UnitId {
        let fqdn = obj_descriptor.to_fqdn();
        UnitId { id: fqdn.next_id() }
    }

    pub fn db_tail() -> UnitId {
        UnitId::unit(&ObjectDescriptor::DbTail)
    }

    pub fn global_index() -> UnitId {
        UnitId::unit(&ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index))
    }

    pub fn vault_unit(vault_name: &str) -> UnitId {
        let vault_desc = ObjectDescriptor::Vault {
            vault_name: vault_name.to_string(),
        };
        UnitId::unit(&vault_desc)
    }

    pub fn meta_pass_unit(vault_name: &str) -> Self {
        let vault_desc = ObjectDescriptor::MetaPassword {
            vault_name: vault_name.to_string(),
        };
        UnitId::unit(&vault_desc)
    }

    pub fn mempool_unit() -> Self {
        UnitId::unit(&ObjectDescriptor::MemPool)
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
            id: self.id.next_id(),
            prev_id: self.id,
            unit_id: self.unit_id,
        }
    }
}

impl GenesisId {
    pub fn genesis(obj_desc: &ObjectDescriptor) -> GenesisId {
        let unit_id = UnitId::unit(obj_desc);
        unit_id.next()
    }

    pub fn global_index_genesis() -> GenesisId {
        GenesisId::genesis(&ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index))
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
            id: self.id.next_id(),
            prev_id: self.id,
            unit_id: self.unit_id,
        }
    }
}
