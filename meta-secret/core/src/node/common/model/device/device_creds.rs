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

    pub fn from_key_manager(
        device_name: DeviceName,
        key_manager: &KeyManager,
    ) -> DeviceCredentials {
        let secret_box = SecretBox::from(key_manager);
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

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use crate::crypto::keys::fixture::KeyManagerFixture;
    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::device::device_creds::DeviceCredentials;

    pub struct DeviceCredentialsFixture {
        pub client: DeviceCredentials,
        pub client_b: DeviceCredentials,
        pub vd: DeviceCredentials,
        pub server: DeviceCredentials,
    }

    impl DeviceCredentialsFixture {
        pub fn from_km(km_fixture: &KeyManagerFixture) -> Self {
            Self {
                client: DeviceCredentials::from_key_manager(
                    DeviceName::client(),
                    &km_fixture.client,
                ),
                client_b: DeviceCredentials::from_key_manager(
                    DeviceName::client_b(),
                    &km_fixture.client_b,
                ),
                vd: DeviceCredentials::from_key_manager(
                    DeviceName::virtual_device(),
                    &km_fixture.vd,
                ),
                server: DeviceCredentials::from_key_manager(
                    DeviceName::server(),
                    &km_fixture.server,
                ),
            }
        }
    }
}
