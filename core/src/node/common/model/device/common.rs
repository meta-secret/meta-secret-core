use std::fmt::Display;

use crypto::utils::generate_uuid_b64_url_enc;

use crate::crypto;
use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::keys::OpenBox;
use crate::crypto::utils::rand_uuid_b64_url_enc;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceId(String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceName(String);

impl DeviceName {
    pub fn server() -> Self {
        DeviceName::from("server")
    }
}

impl From<String> for DeviceName {
    fn from(device_name: String) -> Self {
        DeviceName(device_name)
    }
}

impl From<&str> for DeviceName {
    fn from(device_name: &str) -> Self {
        DeviceName(String::from(device_name))
    }
}

impl DeviceName {
    pub fn generate() -> DeviceName {
        let Base64Text(device_name) = rand_uuid_b64_url_enc();
        DeviceName(device_name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceData {
    pub id: DeviceId,
    pub name: DeviceName,
    pub keys: OpenBox,
}

impl Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.clone())
    }
}

/// Contains only public information about device
impl DeviceData {
    pub fn from(device_name: DeviceName, open_box: OpenBox) -> Self {
        Self {
            name: device_name,
            id: DeviceId::from(&open_box),
            keys: open_box,
        }
    }
}

impl From<&str> for DeviceId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

impl From<&OpenBox> for DeviceId {
    fn from(open_box: &OpenBox) -> Self {
        let dsa_pk = String::from(&open_box.dsa_pk);
        let id = generate_uuid_b64_url_enc(dsa_pk);
        Self(id)
    }
}

#[cfg(test)]
pub mod fixture {
    use crate::node::common::model::device::common::DeviceName;

    pub struct DeviceNameFixture {
        pub client: DeviceName,
        pub vd: DeviceName,
        pub server: DeviceName
    }
    
    impl DeviceNameFixture {
        pub fn generate() -> Self {
            Self {
                client: DeviceName::from("d_client"),
                vd: DeviceName::from("d_vd"),
                server: DeviceName::server(),
            }
        }
    }
}