use crate::crypto::utils;
use crate::node::db::descriptors::object_descriptor::{ObjectName, ObjectType};
use crate::node::db::events::object_id::UnitId;

/// Allows to have access to the global index of all vaults exists across the system.
/// Index + VaultIndex = LinkedHashMap, or linkedList + HaspMap, allows to navigate through the values in the index.
/// Index provides list interface and allows to navigate through elements by their index in the array
/// VaultIndex provides HashMap interface allows to get a vault by its ID
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlobalIndexDescriptor {
    Index,
    /// An id of a vault. We have global index to keep track and being able to iterate over all vaults,
    /// and to be able to check if a particular vault exists we ned to have vault index
    VaultIndex { vault_id: UnitId },
}

impl ObjectType for GlobalIndexDescriptor {
    fn object_type(&self) -> String {
        match self {
            GlobalIndexDescriptor::Index => String::from("GlobalIndex"),
            GlobalIndexDescriptor::VaultIndex { .. } => String::from("VaultIdx")
        }
    }
}

impl ObjectName for GlobalIndexDescriptor {
    fn object_name(&self) -> String {
        match self {
            GlobalIndexDescriptor::Index => String::from("index"),
            GlobalIndexDescriptor::VaultIndex { vault_id } => {
                let json_str = serde_json::to_string(&vault_id.id).unwrap();
                utils::generate_uuid_b64_url_enc(json_str)
            }
        }
    }
}
