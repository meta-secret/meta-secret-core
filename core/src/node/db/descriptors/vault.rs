use std::ops::Add;

use crate::node::common::model::user::UserId;
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ObjectName, ObjectType};

pub enum VaultDescriptor {
    DeviceLog { user_id: UserId },

    VaultLog {
        vault_name: VaultName,
    },
    Vault {
        vault_name: VaultName,
    },
    VaultStatus { user_id: UserId },
}

impl VaultDescriptor {
    pub fn device_log(user_id: UserId) -> ObjectDescriptor {
        ObjectDescriptor::Vault(VaultDescriptor::DeviceLog { user_id })
    }

    pub fn vault_log(vault_name: VaultName) -> ObjectDescriptor {
        ObjectDescriptor::Vault(VaultDescriptor::VaultLog { vault_name })
    }
    
    pub fn vault(vault_name: VaultName) -> ObjectDescriptor {
        ObjectDescriptor::Vault(VaultDescriptor::Vault { vault_name })
    }

    pub fn vault_status(user_id: UserId) -> ObjectDescriptor {
        ObjectDescriptor::Vault(VaultDescriptor::VaultStatus { user_id })
    }
}

impl ObjectType for VaultDescriptor {
    fn object_type(&self) -> String {
        match self {
            VaultDescriptor::DeviceLog { user_id } => {
                String::from("DeviceLog:").add(user_id.device_id.to_string().as_str())
            }
            VaultDescriptor::VaultStatus { user_id } => {
                String::from("VaultStatus:").add(user_id.device_id.to_string().as_str())
            }
            VaultDescriptor::Vault { .. } => String::from("Vault"),
            VaultDescriptor::VaultLog { .. } => String::from("VaultLog"),
        }
    }
}

impl ObjectName for VaultDescriptor {
    fn object_name(&self) -> String {
        match self {
            VaultDescriptor::Vault { vault_name } => vault_name.to_string(),
            VaultDescriptor::DeviceLog { user_id } => user_id.vault_name.to_string(),
            VaultDescriptor::VaultLog { vault_name } => vault_name.to_string(),
            VaultDescriptor::VaultStatus { user_id } => user_id.vault_name.to_string()
        }
    }
}

#[cfg(test)]
pub mod test {
    use std::ops::Add;
    use crate::crypto::keys::{KeyManager, OpenBox};
    use crate::node::common::model::device::DeviceId;
    use crate::node::common::model::vault::VaultName;
    use crate::node::db::descriptors::object_descriptor::{ObjectName, ObjectType};
    use crate::node::db::descriptors::vault::VaultDescriptor;
    use crate::node::db::events::object_id::UnitId;
    use serde_json::json;

    #[test]
    fn test_vault_naming() {
        let vault_name = VaultName::from("test");
        let descriptor = VaultDescriptor::vault(vault_name.clone());
        assert_eq!(descriptor.object_type(), "Vault");
        assert_eq!(descriptor.object_name(), vault_name.to_string());
    }

    #[test]
    fn test_vault_log_naming() {
        let vault_name = VaultName::from("test");
        let descriptor = VaultDescriptor::vault_log(vault_name.clone());
        assert_eq!(descriptor.object_type(), "VaultLog");
        assert_eq!(descriptor.object_name(), vault_name.to_string());
    }

    #[test]
    fn test_device_log_naming() {
        let vault_name = VaultName::from("test_vault");
        let device_id = {
            let secret_box = KeyManager::generate_secret_box();
            DeviceId::from(&OpenBox::from(&secret_box))
        };

        let descriptor = VaultDescriptor::device_log(device_id, vault_name.clone());
        let device_log_type = String::from("DeviceLog:").add(device_id.to_string().as_str());

        assert_eq!(descriptor.object_type(), device_log_type);
        assert_eq!(descriptor.object_name(), vault_name.to_string());

        let unit_id = UnitId::unit(descriptor);

        let id_json = serde_json::to_string(&unit_id.id).unwrap();
        let expected = json!({
            "fqdn": {
                "obj_type": device_log_type,
                "obj_instance": vault_name.to_string()
            },
            "id": 1
        });

        assert_eq!(expected, id_json);
    }
}