use crate::crypto::keys::{KeyManager, OpenBox, SecretBox};
use crate::node::common::model::device::common::{DeviceData, DeviceName};

/// Contains full information about device (private keys and device id)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCredentials {
    pub secret_box: SecretBox,
    pub device: DeviceData,
}

pub struct DeviceCredsBuilder<S> {
    pub creds: S,
}

impl DeviceCredsBuilder<KeyManager> {
    pub fn init(key_manager: KeyManager) -> Self {
        Self { creds: key_manager }
    }

    pub fn generate() -> Self {
        Self {
            creds: KeyManager::generate(),
        }
    }

    pub fn build(self, device_name: DeviceName) -> DeviceCredsBuilder<DeviceCredentials> {
        let secret_box = SecretBox::from(&self.creds);
        let device = DeviceData::from(device_name, OpenBox::from(&secret_box));
        let creds = DeviceCredentials { secret_box, device };

        DeviceCredsBuilder { creds: creds }
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
    use crate::node::common::model::device::device_creds::{DeviceCredentials, DeviceCredsBuilder};

    pub struct DeviceCredentialsFixture {
        pub client: DeviceCredentials,
        pub client_b: DeviceCredentials,
        pub vd: DeviceCredentials,
        pub server: DeviceCredentials,
    }

    impl DeviceCredentialsFixture {
        pub fn from_km(km_fixture: KeyManagerFixture) -> Self {
            let client = DeviceCredsBuilder::init(km_fixture.client)
                .build(DeviceName::client())
                .creds;
            let client_b = DeviceCredsBuilder::init(km_fixture.client_b)
                .build(DeviceName::client_b())
                .creds;
            let vd = DeviceCredsBuilder::init(km_fixture.vd)
                .build(DeviceName::virtual_device())
                .creds;
            let server = DeviceCredsBuilder::init(km_fixture.server)
                .build(DeviceName::server())
                .creds;

            Self {
                client,
                client_b,
                vd,
                server,
            }
        }
    }
}
