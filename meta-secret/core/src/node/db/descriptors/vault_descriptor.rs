use crate::node::common::model::user::common::UserId;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{
    ObjectDescriptor, ObjectName, ObjectType, ToObjectDescriptor,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultDescriptor {
    DeviceLog(UserId),

    VaultLog(VaultName),
    Vault(VaultName),
    VaultMembership(UserId),
}

impl ToObjectDescriptor for VaultDescriptor {
    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::Vault(self)
    }
}

impl VaultDescriptor {
    pub fn device_log(user_id: UserId) -> ObjectDescriptor {
        ObjectDescriptor::Vault(VaultDescriptor::DeviceLog(user_id))
    }

    pub fn vault_log(vault_name: VaultName) -> ObjectDescriptor {
        ObjectDescriptor::Vault(VaultDescriptor::VaultLog(vault_name))
    }

    pub fn vault(vault_name: VaultName) -> ObjectDescriptor {
        ObjectDescriptor::Vault(VaultDescriptor::Vault(vault_name))
    }

    pub fn vault_membership(user_id: UserId) -> ObjectDescriptor {
        ObjectDescriptor::Vault(VaultDescriptor::VaultMembership(user_id))
    }
}

impl ObjectType for VaultDescriptor {
    fn object_type(&self) -> String {
        match self {
            VaultDescriptor::DeviceLog(_) => String::from("DeviceLog"),
            VaultDescriptor::VaultMembership(_) => String::from("VaultStatus"),
            VaultDescriptor::Vault(_) => String::from("Vault"),
            VaultDescriptor::VaultLog(_) => String::from("VaultLog"),
        }
    }
}

impl ObjectName for VaultDescriptor {
    fn object_name(&self) -> String {
        match self {
            VaultDescriptor::Vault(vault_name) => vault_name.to_string(),
            VaultDescriptor::DeviceLog(user_id) => user_id.device_id.to_string(),
            VaultDescriptor::VaultLog(vault_name) => vault_name.to_string(),
            VaultDescriptor::VaultMembership(user_id) => user_id.device_id.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test {
    use serde_json::json;

    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::user::user_creds::UserCredentials;
    use crate::node::common::model::vault::vault::VaultName;
    use crate::node::db::descriptors::object_descriptor::{ObjectName, ObjectType};
    use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
    use crate::node::db::events::object_id::{ObjectId, UnitId};

    #[test]
    fn test_vault_naming() {
        let vault_name = VaultName::test();
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
        let vault_name = VaultName::test();
        let user_creds = UserCredentials::generate(DeviceName::client(), vault_name.clone());
        let user_id = user_creds.user_id();

        let descriptor = VaultDescriptor::device_log(user_id.clone());
        let device_log_type = String::from("DeviceLog");

        println!("{:?}", ObjectId::unit(descriptor.clone()).id_str());

        assert_eq!(descriptor.object_type(), device_log_type);
        assert_eq!(descriptor.object_name(), user_id.device_id.to_string());

        let unit_id = UnitId::unit(&descriptor);

        let id_json = serde_json::to_value(&unit_id.id).unwrap();
        let expected = json!({
            "fqdn": {
                "objType": device_log_type,
                "objInstance": user_id.device_id.to_string()
            },
            "id": 0
        });

        assert_eq!(expected, id_json);
    }
}
