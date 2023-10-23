use serde_derive::{Deserialize, Serialize};

use crate::crypto::utils::IdStrGenerator;
use crate::node::db::events::common::ObjectCreator;
use crate::node::db::events::object_descriptor::{GlobalIndexDescriptor, ObjectDescriptor};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__obj_id")]
pub enum ObjectId {
    Unit(UnitId),
    Genesis(GenesisId),
    Artifact(ArtifactId),
}

/// In category theory, a unit type is a fundamental concept that arises in the study of types and functions.
/// It is often denoted as the unit object, represented by the symbol "1" or "Unit."
/// The unit type serves as a foundational element within category theory,
/// providing a way to represent the absence of information or the presence of a single unique value.
///
/// Same here, Unit is a initial request to create/initialize an object, it's step zero.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitId(String);

impl From<UnitId> for GenesisId {
    fn from(unit_id: UnitId) -> Self {
        Self {
            id: IdStrGenerator::next_id_str(unit_id.0.as_str()),
            unit_id,
        }
    }
}

/// Next step after Unit is Genesis, it's a first step in object initialization,
/// it contains digital signature and public key of the actor (for instance it could be meta secret server) that
/// is responsible to create a persistent object
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenesisId {
    id: String,
    unit_id: UnitId
}

impl From<GenesisId> for ArtifactId {
    fn from(genesis_id: GenesisId) -> Self {
        Self {
            id: IdStrGenerator::next_id_str(genesis_id.id.as_str()),
            prev_id: genesis_id.id,
            unit_id: genesis_id.unit_id,
        }
    }
}

/// Any regular request or update event in the objects' lifetime
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactId {
    id: String,
    prev_id: String,
    unit_id: UnitId
}

/// Generate next artifact from the previous one
impl From<ArtifactId> for ArtifactId {
    fn from(curr: ArtifactId) -> Self {
        Self {
            id: IdStrGenerator::next_id_str(curr.id.as_str()),
            prev_id: curr.id,
            unit_id: curr.unit_id,
        }
    }
}

pub trait Next {
    fn next(&self) -> Self;
}

impl Next for ObjectId {
    fn next( self) -> ObjectId {
        match self {
            ObjectId::Unit(unit_id) => {
                ObjectId::Genesis(GenesisId::from(unit_id))
            }
            ObjectId::Genesis(genesis_id) => {
                ObjectId::Artifact(ArtifactId::from(genesis_id))
            }
            ObjectId::Artifact(artifact_id) => {
                ObjectId::Artifact(ArtifactId::from(artifact_id))
            }
        }
    }
}

impl ObjectId {
    pub fn unit_id(&self) -> UnitId {
        match self {
            ObjectId::Unit(unit_id) => unit_id.clone(),
            ObjectId::Genesis(genesis_id) => genesis_id.unit_id.clone(),
            ObjectId::Artifact(artifact_id) => artifact_id.unit_id.clone()
        }
    }

    pub fn id_str(&self) -> String {
        match self {
            ObjectId::Unit(unit_id) => unit_id.0.clone(),
            ObjectId::Genesis(genesis_id) => genesis_id.id.clone(),
            ObjectId::Artifact(artifact_id) => artifact_id.id.clone(),
        }
    }

    pub fn db_tail() -> ObjectId {
        ObjectId::unit(&ObjectDescriptor::DbTail)
    }

    pub fn global_index_unit() -> ObjectId {
        ObjectId::unit(&ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index))
    }

    pub fn global_index_genesis() -> ObjectId {
        ObjectId::genesis(&ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index))
    }

    pub fn vault_unit(vault_name: &str) -> Self {
        let vault_desc = ObjectDescriptor::Vault {
            vault_name: vault_name.to_string(),
        };
        ObjectId::unit(&vault_desc)
    }

    pub fn meta_pass_unit(vault_name: &str) -> Self {
        let vault_desc = ObjectDescriptor::MetaPassword {
            vault_name: vault_name.to_string(),
        };
        ObjectId::unit(&vault_desc)
    }

    pub fn mempool_unit() -> Self {
        ObjectId::unit(&ObjectDescriptor::MemPool)
    }
}

impl ObjectCreator<&ObjectDescriptor> for ObjectId {
    fn unit(obj_descriptor: &ObjectDescriptor) -> Self {
        let unit_id = obj_descriptor.to_id();
        Self::Unit(UnitId(unit_id))
    }

    fn genesis(obj_desc: &ObjectDescriptor) -> Self {
        Self::unit(obj_desc).next()
    }
}

#[cfg(test)]
mod test {
    use crate::node::db::events::common::ObjectCreator;
    use crate::node::db::events::object_descriptor::ObjectDescriptor;
    use crate::node::db::events::object_id::ObjectId;

    #[test]
    fn json_parsing_test() {
        let obj_id = ObjectId::unit(&ObjectDescriptor::Vault { vault_name: String::from("test")});
        let obj_id_json = serde_json::to_string(&obj_id).unwrap();
        println!("{}", obj_id_json);
    }
}
