use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{
    ObjectDescriptor, ObjectName, ObjectType, ToObjectDescriptor,
};
use crate::node::db::events::global_index_event::GlobalIndexObject;
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
    /// and to be able to check if a particular vault exists we need to have vault index
    VaultIndex {
        vault_id: UnitId,
    },
}

impl ObjectType for GlobalIndexDescriptor {
    fn object_type(&self) -> String {
        match self {
            GlobalIndexDescriptor::Index => String::from("GlobalIndex"),
            GlobalIndexDescriptor::VaultIndex { .. } => String::from("VaultIdx"),
        }
    }
}

impl ObjectName for GlobalIndexDescriptor {
    fn object_name(&self) -> String {
        match self {
            GlobalIndexDescriptor::Index => String::from("index"),
            GlobalIndexDescriptor::VaultIndex { vault_id } => {
                let id = [
                    vault_id.id.fqdn.obj_type.clone(),
                    vault_id.id.fqdn.obj_instance.clone(),
                    vault_id.id.id.to_string(),
                ]
                .join("|");

                id
            }
        }
    }
}

impl GlobalIndexDescriptor {
    pub fn vault_index(vault_name: VaultName) -> GlobalIndexDescriptor {
        let vault_id = UnitId::vault_unit(vault_name);
        GlobalIndexDescriptor::VaultIndex { vault_id }
    }
}

impl ToObjectDescriptor for GlobalIndexDescriptor {
    type EventType = GlobalIndexObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::GlobalIndex(self)
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use crate::node::common::model::vault::vault::VaultName;
    use crate::node::db::descriptors::global_index_descriptor::GlobalIndexDescriptor;
    use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;

    #[test]
    fn test_global_index() -> anyhow::Result<()> {
        let vault_name = VaultName::from("test_vault");

        let vault_index_json = {
            let vault_index = GlobalIndexDescriptor::vault_index(vault_name.clone())
                .to_obj_desc()
                .fqdn();
            serde_json::to_value(vault_index)?
        };

        let expected = json!({
            "objType":"VaultIdx",
            "objInstance": "Vault|test_vault|0"
        });
        assert_eq!(expected, vault_index_json);

        Ok(())
    }
}
