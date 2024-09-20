use crate::crypto::keys::{KeyManager, OpenBox, SecretBox};
use crate::node::common::model::device::common::{DeviceData, DeviceName};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCredentials {
    pub secret_box: SecretBox,
    pub device: DeviceData,
}

/// Contains full information about device (private keys and device id)
impl DeviceCredentials {
    pub fn generate(device_name: DeviceName) -> DeviceCredentials {
        let secret_box = KeyManager::generate_secret_box();
        let device = DeviceData::from(device_name, OpenBox::from(&secret_box));
        DeviceCredentials { secret_box, device }
    }

    pub fn key_manager(&self) -> anyhow::Result<KeyManager> {
        let key_manager = KeyManager::try_from(&self.secret_box)?;
        Ok(key_manager)
    }
}

#[cfg(test)]
pub mod fixture {
    use crate::node::common::model::device::common::fixture::DeviceNameFixture;
    use crate::node::common::model::device::device_creds::DeviceCredentials;

    pub struct DeviceCredentialsFixture {
        pub client: DeviceCredentials,
        pub vd: DeviceCredentials,
        pub server: DeviceCredentials
    }

    impl From<&DeviceNameFixture> for DeviceCredentialsFixture {
        fn from(device_name_fixture: &DeviceNameFixture) -> Self {
            Self {
                client: DeviceCredentials::generate(device_name_fixture.client.clone()),
                vd: DeviceCredentials::generate(device_name_fixture.vd.clone()),
                server: DeviceCredentials::generate(device_name_fixture.server.clone()),
            }
        }
    }

    impl DeviceCredentialsFixture {
        pub fn generate() -> Self {
            DeviceCredentialsFixture::from(&DeviceNameFixture::generate())
        }
    }
}
