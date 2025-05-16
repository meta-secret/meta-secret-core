use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::device::device_creds::{SecureDeviceCreds};
use crate::node::common::model::user::user_creds::{SecureUserCreds};
use crate::node::db::descriptors::creds::{DeviceCredsDescriptor, UserCredsDescriptor};
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ArtifactId;
use anyhow::{anyhow, Error};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCredsObject(pub KvLogEvent<SecureDeviceCreds>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCredsObject(pub KvLogEvent<SecureUserCreds>);

impl From<SecureDeviceCreds> for DeviceCredsObject {
    fn from(creds: SecureDeviceCreds) -> Self {
        DeviceCredsObject(KvLogEvent {
            key: KvKey::from(DeviceCredsDescriptor),
            value: creds,
        })
    }
}

impl From<SecureUserCreds> for UserCredsObject {
    fn from(creds: SecureUserCreds) -> Self {
        UserCredsObject(KvLogEvent {
            key: KvKey::from(UserCredsDescriptor),
            value: creds,
        })
    }
}

impl ObjIdExtractor for DeviceCredsObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}

impl KeyExtractor for DeviceCredsObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}

impl ToGenericEvent for DeviceCredsObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::DeviceCreds(self)
    }
}

impl TryFrom<GenericKvLogEvent> for DeviceCredsObject {
    type Error = Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::DeviceCreds(device_creds) = event {
            Ok(device_creds)
        } else {
            Err(anyhow!("Invalid device credentials event type"))
        }
    }
}

impl DeviceCredsObject {
    pub fn device(&self) -> DeviceData {
        self.0.value.device.clone()
    }

    pub fn value(self) -> SecureDeviceCreds {
        self.0.value
    }
}

impl ObjIdExtractor for UserCredsObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}

impl KeyExtractor for UserCredsObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}

impl ToGenericEvent for UserCredsObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::UserCreds(self)
    }
}

impl TryFrom<GenericKvLogEvent> for UserCredsObject {
    type Error = Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::UserCreds(user_creds) = event {
            Ok(user_creds)
        } else {
            Err(anyhow!("Invalid user credentials event type"))
        }
    }
}

impl UserCredsObject {
    pub fn device(&self) -> DeviceData {
        self.0.value.device_creds.device.clone()
    }

    pub fn value(&self) -> SecureUserCreds {
        self.0.value.clone()
    }

    pub fn device_creds(&self) -> SecureDeviceCreds {
        self.0.value.device_creds.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::key_pair::KeyPair;
use crate::node::common::model::device::device_creds::DeviceCreds;
    use crate::node::common::model::user::user_creds::UserCreds;
    use super::*;
    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::device::device_creds::DeviceCredsBuilder;
    use crate::node::common::model::user::user_creds::UserCredsBuilder;
    use crate::node::common::model::vault::vault::VaultName;
    use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
    use anyhow::Result;
    use crate::crypto::key_pair::TransportDsaKeyPair;

    fn create_test_device_credentials() -> DeviceCreds {
        let device_name = DeviceName::from("test_device");
        DeviceCredsBuilder::generate().build(device_name).creds
    }

    fn create_test_user_credentials() -> UserCreds {
        let device_creds = create_test_device_credentials();
        let vault_name = VaultName::from("test_vault");
        UserCredsBuilder::init(device_creds).build(vault_name).creds
    }

    #[test]
    fn test_device_creds_object_key_extraction() -> Result<()> {
        let device_creds = create_test_device_credentials();
        let master_pk = TransportDsaKeyPair::generate().sk().pk()?;
        let secure_device_creds = SecureDeviceCreds::build(device_creds.clone(), master_pk)?;
        
        let device_creds_obj = DeviceCredsObject::from(secure_device_creds);

        let key = device_creds_obj.key();
        let obj_id = device_creds_obj.obj_id();

        assert_eq!(key.obj_id, obj_id);

        assert!(matches!(key.obj_desc, ObjectDescriptor::DeviceCreds(_)));
        Ok(())
    }

    #[test]
    fn test_user_creds_object_key_extraction() -> Result<()> {
        let user_creds = create_test_user_credentials();
        let master_pk = TransportDsaKeyPair::generate().sk().pk()?;
        let secure_user_creds = SecureUserCreds::build(user_creds, master_pk)?;
        let user_creds_obj = UserCredsObject::from(secure_user_creds);

        let key = user_creds_obj.key();
        let obj_id = user_creds_obj.obj_id();

        assert_eq!(key.obj_id, obj_id);
        assert!(matches!(key.obj_desc, ObjectDescriptor::UserCreds(_)));
        Ok(())
    }

    #[test]
    fn test_device_creds_object_to_generic_event() -> Result<()> {
        let device_creds = create_test_device_credentials();
        let master_pk = TransportDsaKeyPair::generate().sk().pk()?;
        let secure_device_creds = SecureDeviceCreds::build(device_creds.clone(), master_pk)?;
        let device_creds_obj = DeviceCredsObject::from(secure_device_creds);
        let generic_event = device_creds_obj.clone().to_generic();

        // Try to convert it back
        let recovered_object = DeviceCredsObject::try_from(generic_event)?;

        // Check that the key is preserved
        let original_key = device_creds_obj.key();
        let recovered_key = recovered_object.key();

        assert_eq!(original_key.obj_id, recovered_key.obj_id);
        assert!(matches!(
            recovered_key.obj_desc,
            ObjectDescriptor::DeviceCreds(_)
        ));
        
        Ok(())
    }

    #[test]
    fn test_user_creds_object_to_generic_event() -> Result<()> {
        let user_creds = create_test_user_credentials();
        let master_pk = TransportDsaKeyPair::generate().sk().pk()?;
        let secure_user_creds = SecureUserCreds::build(user_creds, master_pk)?;
        let user_creds_obj = UserCredsObject::from(secure_user_creds);
        let generic_event = user_creds_obj.clone().to_generic();

        // Try to convert it back
        let recovered_object = UserCredsObject::try_from(generic_event)?;

        // Check that the key is preserved
        let original_key = user_creds_obj.key();
        let recovered_key = recovered_object.key();

        assert_eq!(original_key.obj_id, recovered_key.obj_id);

        assert!(matches!(
            recovered_key.obj_desc,
            ObjectDescriptor::UserCreds(_)
        ));
        
        Ok(())
    }

    #[test]
    fn test_try_from_wrong_event_type() -> Result<()> {
        let device_creds = create_test_device_credentials();
        let master_pk = TransportDsaKeyPair::generate().sk().pk()?;
        let secure_device_creds = SecureDeviceCreds::build(device_creds.clone(), master_pk)?;
        let device_creds_obj = DeviceCredsObject::from(secure_device_creds);
        let generic_event = device_creds_obj.to_generic();

        // Try to convert the device creds event to user creds - should fail
        let result = UserCredsObject::try_from(generic_event);
        assert!(result.is_err());
        
        Ok(())
    }
}
