use std::fmt::Display;
use wasm_bindgen::prelude::wasm_bindgen;
use crypto::utils::generate_uuid_b64_url_enc;

use crate::crypto;
use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::keys::OpenBox;
use crate::crypto::utils::rand_uuid_b64_url_enc;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct DeviceId(String);
#[wasm_bindgen]
impl DeviceId {
    pub fn as_str(&self) -> String {
        self.0.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct DeviceName(String);

#[wasm_bindgen]
impl DeviceName {
    pub fn server() -> Self {
        DeviceName::from("server_device")
    }
    
    pub fn virtual_device() -> Self {
        DeviceName::from("vd_device")
    }

    pub fn client() -> Self {
        DeviceName::from("client_device")
    }
    
    pub fn as_str(&self) -> String {
        self.0.clone()
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
#[wasm_bindgen(getter_with_clone)]
pub struct DeviceData {
    pub device_id: DeviceId,
    pub device_name: DeviceName,
    pub keys: OpenBox,
}

/// Contains only public information about device
impl DeviceData {
    pub fn from(device_name: DeviceName, open_box: OpenBox) -> Self {
        Self {
            device_name,
            device_id: DeviceId::from(&open_box),
            keys: open_box,
        }
    }
}

impl Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.clone())
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
