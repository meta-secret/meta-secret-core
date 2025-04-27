use crate::node::db::descriptors::object_descriptor::{
    ObjectDescriptor, ObjectName, ObjectType, ToObjectDescriptor,
};
use crate::node::db::events::local_event::{DeviceCredsObject, UserCredsObject};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCredsDescriptor;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCredsDescriptor;

impl ToObjectDescriptor for DeviceCredsDescriptor {
    type EventType = DeviceCredsObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::DeviceCreds(self)
    }
}

impl ToObjectDescriptor for UserCredsDescriptor {
    type EventType = UserCredsObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::UserCreds(self)
    }
}

impl ObjectType for DeviceCredsDescriptor {
    fn object_type(&self) -> String {
        String::from("DeviceCreds")
    }
}

impl ObjectType for UserCredsDescriptor {
    fn object_type(&self) -> String {
        String::from("UserCreds")
    }
}

impl ObjectName for DeviceCredsDescriptor {
    fn object_name(&self) -> String {
        String::from("index")
    }
}

impl ObjectName for UserCredsDescriptor {
    fn object_name(&self) -> String {
        String::from("index")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::common::model::IdString;

    #[test]
    fn test_device_creds_descriptor_to_obj_desc() {
        let device_desc = DeviceCredsDescriptor;
        let obj_desc = device_desc.to_obj_desc();

        match obj_desc {
            ObjectDescriptor::DeviceCreds(desc) => {
                assert_eq!(desc.object_type(), "DeviceCreds");
                assert_eq!(desc.object_name(), "index");
            }
            _ => panic!("Expected DeviceCreds variant"),
        }
    }

    #[test]
    fn test_user_creds_descriptor_to_obj_desc() {
        let user_desc = UserCredsDescriptor;
        let obj_desc = user_desc.to_obj_desc();

        match obj_desc {
            ObjectDescriptor::UserCreds(desc) => {
                assert_eq!(desc.object_type(), "UserCreds");
                assert_eq!(desc.object_name(), "index");
            }
            _ => panic!("Expected UserCreds variant"),
        }
    }

    #[test]
    fn test_object_fqdn() {
        // Test DeviceCredsDescriptor
        let device_desc = DeviceCredsDescriptor;
        let obj_desc = device_desc.to_obj_desc();
        let fqdn = obj_desc.fqdn();

        assert_eq!(fqdn.obj_type, "DeviceCreds");
        assert_eq!(fqdn.obj_instance, "index");
        assert_eq!(fqdn.id_str(), "DeviceCreds:index");

        // Test UserCredsDescriptor
        let user_desc = UserCredsDescriptor;
        let obj_desc = user_desc.to_obj_desc();
        let fqdn = obj_desc.fqdn();

        assert_eq!(fqdn.obj_type, "UserCreds");
        assert_eq!(fqdn.obj_instance, "index");
        assert_eq!(fqdn.id_str(), "UserCreds:index");
    }
}
