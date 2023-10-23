use serde_derive::{Deserialize, Serialize};

use crate::crypto::utils;
use crate::node::db::events::common::ObjectCreator;
use crate::node::db::events::object_descriptor::{GlobalIndexDescriptor, ObjectDescriptor};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__obj_id")]
pub enum ObjectId {
    /// In category theory, a unit type is a fundamental concept that arises in the study of types and functions.
    /// It is often denoted as the unit object, represented by the symbol "1" or "Unit."
    /// The unit type serves as a foundational element within category theory,
    /// providing a way to represent the absence of information or the presence of a single unique value.
    ///
    /// Same here, Unit is a initial request to create/initialize an object, it's step zero.
    Unit { id: String },
    /// Next step after Unit is Genesis, it's a first step in object initialization,
    /// it contains digital signature and public key of the actor (for instance it could be meta secret server) that
    /// is responsible to create a persistent object
    Genesis { id: String, unit_id: String },
    /// Any regular request or update event in the objects' lifetime
    Artifact {
        id: String,
        prev_id: String,
        unit_id: String,
    },
}

/// In category theory, a unit type is a fundamental concept that arises in the study of types and functions.
/// It is often denoted as the unit object, represented by the symbol "1" or "Unit."
/// The unit type serves as a foundational element within category theory,
/// providing a way to represent the absence of information or the presence of a single unique value.
///
/// Same here, Unit is a initial request to create/initialize an object, it's step zero.
pub struct UnitId(String);

pub struct GenesisId {
    id: String,
    unit_id: UnitId
}

pub struct ArtifactId {
    id: String,
    prev_id: String,
    unit_id: UnitId
}

#[derive(Debug)]
pub struct IdStr {
    pub id: String,
}

pub trait IdGen {
    fn next(&self) -> Self;
}

impl IdGen for ObjectId {
    fn next(&self) -> ObjectId {
        let next_id_str = utils::to_id(self.id_str().as_str());

        match self {
            ObjectId::Unit { id } => ObjectId::Genesis {
                id: next_id_str,
                unit_id: id.clone(),
            },
            ObjectId::Genesis { id, unit_id } => ObjectId::Artifact {
                id: next_id_str,
                prev_id: id.clone(),
                unit_id: unit_id.clone(),
            },
            ObjectId::Artifact { id, unit_id, .. } => ObjectId::Artifact {
                id: next_id_str,
                prev_id: id.clone(),
                unit_id: unit_id.clone(),
            },
        }
    }
}

impl From<&ObjectId> for IdStr {
    fn from(obj_id: &ObjectId) -> IdStr {
        IdStr { id: obj_id.id_str() }
    }
}

impl ObjectId {
    pub fn unit_id(&self) -> ObjectId {
        match self {
            ObjectId::Unit { .. } => self.clone(),
            ObjectId::Genesis { unit_id, .. } => ObjectId::Unit { id: unit_id.clone() },
            ObjectId::Artifact { unit_id, .. } => Self::Unit { id: unit_id.clone() },
        }
    }

    pub fn id_str(&self) -> String {
        match self {
            ObjectId::Genesis { id, .. } => id.clone(),
            ObjectId::Artifact { id, .. } => id.clone(),
            ObjectId::Unit { id } => id.clone(),
        }
    }

    pub fn is_unit(&self) -> bool {
        match self {
            ObjectId::Unit { .. } => true,
            ObjectId::Genesis { .. } => false,
            ObjectId::Artifact { .. } => false,
        }
    }

    pub fn is_genesis(&self) -> bool {
        match self {
            ObjectId::Unit { .. } => false,
            ObjectId::Genesis { .. } => true,
            ObjectId::Artifact { .. } => false,
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
        Self::Unit { id: unit_id }
    }

    fn genesis(obj_desc: &ObjectDescriptor) -> Self {
        Self::unit(obj_desc).next()
    }
}

#[cfg(test)]
mod test {
    use crate::node::db::events::object_id::ObjectId;

    #[test]
    fn json_parsing_test() {
        let obj_id = ObjectId::Unit { id: "test".to_string() };
        let obj_id_json = serde_json::to_string(&obj_id).unwrap();
        println!("{}", obj_id_json);
    }
}
