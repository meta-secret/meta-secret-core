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
}

impl DeviceCredentials {
    pub fn key_manager(&self) -> anyhow::Result<KeyManager> {
        let key_manager = KeyManager::try_from(&self.secret_box)?;
        Ok(key_manager)
    }
}

#[cfg(test)]
pub mod fixture {
    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::device::device_creds::DeviceCredentials;

    pub struct DeviceCredentialsFixture {
        pub client: DeviceCredentials,
        pub client_b: DeviceCredentials,
        pub vd: DeviceCredentials,
        pub server: DeviceCredentials,
    }

    impl DeviceCredentialsFixture {
        pub fn generate() -> Self {
            Self {
                client: DeviceCredentials::generate(DeviceName::client()),
                client_b: DeviceCredentials::generate(DeviceName::client_b()),
                vd: DeviceCredentials::generate(DeviceName::virtual_device()),
                server: DeviceCredentials::generate(DeviceName::server()),
            }
        }
    }
}
