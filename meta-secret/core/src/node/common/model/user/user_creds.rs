use crate::crypto::keys::TransportPk;
use crate::node::common::model::device::common::{DeviceData, DeviceId};
use crate::node::common::model::device::device_creds::{DeviceCreds, SecureDeviceCreds};
use crate::node::common::model::user::common::{UserData, UserId};
use crate::node::common::model::vault::vault::VaultName;
use anyhow::Result;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureUserCreds {
    pub vault_name: VaultName,
    pub device_creds: SecureDeviceCreds,
}

impl SecureUserCreds {
    
    pub fn build(user_creds: UserCreds, master_pk: TransportPk) -> Result<Self> {
        let secure_device_creds = SecureDeviceCreds::build(user_creds.device_creds, master_pk)?;

        // Create secure user credentials with the secure device credentials
        Ok(SecureUserCreds {
            vault_name: user_creds.vault_name.clone(),
            device_creds: secure_device_creds,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCreds {
    pub vault_name: VaultName,
    pub device_creds: DeviceCreds,
}

impl UserCreds {
    pub fn device(&self) -> DeviceData {
        self.device_creds.device.clone()
    }

    pub fn user(&self) -> UserData {
        UserData {
            vault_name: self.vault_name.clone(),
            device: self.device(),
        }
    }

    pub fn user_id(&self) -> UserId {
        UserId {
            vault_name: self.vault_name.clone(),
            device_id: self.device().device_id.clone(),
        }
    }

    pub fn device_id(&self) -> &DeviceId {
        &self.device_creds.device.device_id
    }
}

pub struct UserCredsBuilder<Creds> {
    pub creds: Creds,
}

impl UserCredsBuilder<DeviceCreds> {
    pub fn init(creds: DeviceCreds) -> Self {
        UserCredsBuilder { creds }
    }

    pub fn build(self, vault_name: VaultName) -> UserCredsBuilder<UserCreds> {
        let user_creds = UserCreds {
            vault_name,
            device_creds: self.creds,
        };
        UserCredsBuilder { creds: user_creds }
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
    use crate::node::common::model::user::user_creds::{UserCreds, UserCredsBuilder};
    use crate::node::common::model::vault::vault::VaultName;

    pub struct UserCredentialsFixture {
        pub client: UserCreds,
        pub vd: UserCreds,
        pub client_b: UserCreds,
    }

    impl UserCredentialsFixture {
        pub fn client_device_name(&self) -> DeviceName {
            self.client.device_creds.device.device_name.clone()
        }
    }

    impl UserCredentialsFixture {
        pub fn from(device_creds: &DeviceCredentialsFixture) -> Self {
            Self {
                client: UserCredsBuilder::init(device_creds.client.clone())
                    .build(VaultName::test())
                    .creds,
                vd: UserCredsBuilder::init(device_creds.vd.clone())
                    .build(VaultName::test())
                    .creds,
                client_b: UserCredsBuilder::init(device_creds.client_b.clone())
                    .build(VaultName::test())
                    .creds,
            }
        }
    }
}
