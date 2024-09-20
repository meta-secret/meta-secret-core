use crate::node::common::model::device::common::{DeviceData, DeviceName};
use crate::node::common::model::device::device_creds::DeviceCredentials;
use crate::node::common::model::user::common::{UserData, UserId};
use crate::node::common::model::vault::VaultName;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCredentials {
    pub vault_name: VaultName,
    pub device_creds: DeviceCredentials,
}

impl UserCredentials {
    pub fn from(device_creds: DeviceCredentials, vault_name: VaultName) -> UserCredentials {
        UserCredentials {
            vault_name,
            device_creds,
        }
    }

    pub fn generate(device_name: DeviceName, vault_name: VaultName) -> UserCredentials {
        UserCredentials {
            vault_name,
            device_creds: DeviceCredentials::generate(device_name),
        }
    }

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
            device_id: self.device().id.clone(),
        }
    }
}

#[cfg(test)]
pub mod fixture {
    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
    use crate::node::common::model::user::user_creds::UserCredentials;
    use crate::node::common::model::vault::fixture::VaultNameFixture;

    pub struct UserCredentialsFixture {
        pub client: UserCredentials,
        pub vd: UserCredentials,
        pub server: UserCredentials
    }

    impl UserCredentialsFixture {
        pub fn client_device_name(&self) -> DeviceName {
            self.client.device_creds.device.name.clone()
        }
    }

    impl UserCredentialsFixture {
        pub fn generate() -> Self {
            UserCredentialsFixture::from(
                &DeviceCredentialsFixture::generate(),
                &VaultNameFixture::generate()
            )
        }

        pub fn from(device_creds: &DeviceCredentialsFixture, vault_name: &VaultNameFixture) -> Self {
            Self {
                client: UserCredentials::from(device_creds.client.clone(), vault_name.client.clone()),
                vd: UserCredentials::from(device_creds.vd.clone(), vault_name.vd.clone()),
                server: UserCredentials::from(device_creds.server.clone(), vault_name.server.clone()),
            }
        }
    }
}
