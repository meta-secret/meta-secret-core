use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::keys::{KeyManager, OpenBox, SecretBox, SecureSecretBox, TransportPk, TransportSk};
use crate::node::common::model::crypto::aead::AeadPlainText;
use crate::node::common::model::crypto::channel::CommunicationChannel;
use crate::node::common::model::device::common::{DeviceData, DeviceName};
use anyhow::Result;

/// Contains full information about device (private keys and device id)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureDeviceCreds {
    pub secret_box: SecureSecretBox,
    pub device: DeviceData,
}

impl SecureDeviceCreds {
    pub fn decrypt(self, sk: &TransportSk) -> Result<DeviceCreds> {
        let encrypted_secret_box = self.secret_box.secret_box;
        let secret_box_json = encrypted_secret_box.decrypt(sk)?;
        let secret_box_str = String::try_from(&secret_box_json.msg)?;

        let secret_box: SecretBox = serde_json::from_str(&secret_box_str)?;

        let device_creds = DeviceCreds {
            secret_box,
            device: self.device,
        };
        Ok(device_creds)
    }
}

impl SecureDeviceCreds {
    
    pub fn build(device_creds: DeviceCreds, master_pk: TransportPk) -> Result<Self> {
        let device_creds_json = serde_json::to_string(&device_creds)?;
        let channel = CommunicationChannel::single_device(master_pk).to_channel();

        let plain_secret_box = AeadPlainText {
            msg: Base64Text::from(device_creds_json),
            channel,
        };

        let secure_secret_box = plain_secret_box.encrypt()?;

        Ok(SecureDeviceCreds {
            secret_box: SecureSecretBox {
                secret_box: secure_secret_box,
            },
            device: device_creds.device,
        })
    }
}

/// Contains full information about device (private keys and device id)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCreds {
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

    pub fn build(self, device_name: DeviceName) -> DeviceCredsBuilder<DeviceCreds> {
        let secret_box = SecretBox::from(&self.creds);
        let device = DeviceData::from(device_name, OpenBox::from(&secret_box));
        let creds = DeviceCreds { secret_box, device };

        DeviceCredsBuilder { creds }
    }
}

impl DeviceCreds {
    pub fn key_manager(&self) -> anyhow::Result<KeyManager> {
        let key_manager = KeyManager::try_from(&self.secret_box)?;
        Ok(key_manager)
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use crate::crypto::key_pair::{KeyPair, TransportDsaKeyPair};
    use crate::crypto::keys::TransportSk;
    use crate::crypto::keys::fixture::KeyManagerFixture;
    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::device::device_creds::{DeviceCreds, DeviceCredsBuilder};

    pub struct DeviceCredentialsFixture {
        pub client: DeviceCreds,
        pub client_b: DeviceCreds,
        pub vd: DeviceCreds,
        pub server: DeviceCreds,

        pub client_master_key: TransportSk,
        pub client_b_master_key: TransportSk,
        pub vd_master_key: TransportSk,
        pub server_master_key: TransportSk,
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

                client_master_key: TransportDsaKeyPair::generate().sk(),
                client_b_master_key: TransportDsaKeyPair::generate().sk(),
                vd_master_key: TransportDsaKeyPair::generate().sk(),
                server_master_key: TransportDsaKeyPair::generate().sk(),
            }
        }
    }
}
