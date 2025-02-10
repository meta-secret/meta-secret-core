use crate::node::common::model::user::common::UserId;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{
    ObjectDescriptor, ObjectName, ObjectType, ToObjectDescriptor,
};
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::VaultLogObject;
use crate::node::db::events::vault::vault_status::VaultStatusObject;
use derive_more::From;

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLogDescriptor(UserId);

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultLogDescriptor(VaultName);

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultDescriptor(VaultName);

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatusDescriptor(UserId);

impl ToObjectDescriptor for DeviceLogDescriptor {
    type EventType = DeviceLogObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::DeviceLog(self)
    }
}

impl ObjectType for DeviceLogDescriptor {
    fn object_type(&self) -> String {
        String::from("DeviceLog")
    }
}

impl ObjectName for DeviceLogDescriptor {
    fn object_name(&self) -> String {
        self.0.device_id.to_string()
    }
}

impl ToObjectDescriptor for VaultLogDescriptor {
    type EventType = VaultLogObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::VaultLog(self)
    }
}

impl ObjectType for VaultLogDescriptor {
    fn object_type(&self) -> String {
        String::from("VaultLog")
    }
}

impl ObjectName for VaultLogDescriptor {
    fn object_name(&self) -> String {
        self.0.to_string()
    }
}

impl ToObjectDescriptor for VaultDescriptor {
    type EventType = VaultObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::Vault(self)
    }
}

impl ObjectType for VaultDescriptor {
    fn object_type(&self) -> String {
        String::from("Vault")
    }
}

impl ObjectName for VaultDescriptor {
    fn object_name(&self) -> String {
        self.0.to_string()
    }
}

impl ToObjectDescriptor for VaultStatusDescriptor {
    type EventType = VaultStatusObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::VaultStatus(self)
    }
}

impl ObjectType for VaultStatusDescriptor {
    fn object_type(&self) -> String {
        String::from("VaultStatus")
    }
}

impl ObjectName for VaultStatusDescriptor {
    fn object_name(&self) -> String {
        self.0.device_id.to_string()
    }
}

#[cfg(test)]
pub mod test {
    use serde_json::json;

    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::user::user_creds::UserCredentials;
    use crate::node::common::model::vault::vault::VaultName;
    use crate::node::db::descriptors::object_descriptor::{
        ObjectName, ObjectType, ToObjectDescriptor,
    };
    use crate::node::db::descriptors::vault_descriptor::{
        DeviceLogDescriptor, VaultDescriptor, VaultLogDescriptor,
    };
    use crate::node::db::events::object_id::ArtifactId;

    #[test]
    fn test_vault_naming() {
        let vault_name = VaultName::test();
        let descriptor = VaultDescriptor(vault_name.clone());
        assert_eq!(descriptor.object_type(), "Vault");
        assert_eq!(descriptor.object_name(), vault_name.to_string());
    }

    #[test]
    fn test_vault_log_naming() {
        let vault_name = VaultName::from("test");
        let descriptor = VaultLogDescriptor(vault_name.clone());
        assert_eq!(descriptor.object_type(), "VaultLog");
        assert_eq!(descriptor.object_name(), vault_name.to_string());
    }

    #[test]
    fn test_device_log_naming() {
        let vault_name = VaultName::test();
        let user_creds = UserCredentials::generate(DeviceName::client(), vault_name.clone());
        let user_id = user_creds.user_id();

        let descriptor = DeviceLogDescriptor(user_id.clone());
        let device_log_type = String::from("DeviceLog");

        println!("{:?}", descriptor.clone().to_obj_desc());

        assert_eq!(descriptor.object_type(), device_log_type);
        assert_eq!(descriptor.object_name(), user_id.device_id.to_string());

        let unit_id = ArtifactId::from(descriptor);

        let id_json = serde_json::to_value(&unit_id).unwrap();
        let expected = json!({
            "fqdn": {
                "objType": device_log_type,
                "objInstance": user_id.device_id.to_string()
            },
            "id": {
                "curr": 1,
                "prev": 0
            }
        });

        assert_eq!(expected, id_json);
    }
}
