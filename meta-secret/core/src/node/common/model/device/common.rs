use serde::{Deserialize, Serialize};
use std::fmt::Display;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::crypto::keys::OpenBox;
use crate::crypto::utils::{U64IdUrlEnc, UuidUrlEnc};
use crate::node::common::model::device::device_link::LoopbackDeviceLink;
use crate::node::common::model::IdString;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(transparent)]
#[wasm_bindgen(getter_with_clone)]
pub struct DeviceId(pub U64IdUrlEnc);

impl IdString for DeviceId {
    fn id_str(self) -> String {
        self.0.id_str()
    }
}

#[wasm_bindgen]
impl DeviceId {
    pub fn wasm_id_str(&self) -> String {
        self.clone().id_str()
    }
}

impl DeviceId {
    pub fn loopback(self) -> LoopbackDeviceLink {
        LoopbackDeviceLink::from(self)
    }
}

impl Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.clone().id_str())
    }
}

impl From<&OpenBox> for DeviceId {
    fn from(open_box: &OpenBox) -> Self {
        open_box.transport_pk.to_device_id()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    pub fn client_b() -> Self {
        DeviceName::from("client_device_b")
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
        let uuid = UuidUrlEnc::generate();
        DeviceName(uuid.id_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(transparent)]
#[wasm_bindgen(getter_with_clone)]
pub struct DeviceType(String);

#[wasm_bindgen]
impl DeviceType {
    pub fn android() -> Self {
        DeviceType::from("Android")
    }

    pub fn iphone() -> Self {
        DeviceType::from("iPhone")
    }

    pub fn tablet() -> Self {
        DeviceType::from("Tablet")
    }

    pub fn web() -> Self {
        DeviceType::from("Web")
    }

    pub fn cli() -> Self {
        DeviceType::from("CLI")
    }

    pub fn desktop() -> Self {
        DeviceType::from("Desktop")
    }

    pub fn other() -> Self {
        DeviceType::from("Other")
    }

    pub fn as_str(&self) -> String {
        self.0.clone()
    }
}

impl Default for DeviceType {
    fn default() -> Self {
        DeviceType::other()
    }
}

impl From<String> for DeviceType {
    fn from(device_type: String) -> Self {
        DeviceType(device_type)
    }
}

impl From<&str> for DeviceType {
    fn from(device_type: &str) -> Self {
        DeviceType(String::from(device_type))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct DeviceData {
    pub device_id: DeviceId,
    pub device_name: DeviceName,
    pub device_type: DeviceType,
    pub keys: OpenBox,
}

/// Contains only public information about device
impl DeviceData {
    pub fn from_with_type(device_name: DeviceName, device_type: DeviceType, open_box: OpenBox) -> Self {
        Self {
            device_name,
            device_type,
            device_id: DeviceId::from(&open_box),
            keys: open_box,
        }
    }

    pub fn from(device_name: DeviceName, open_box: OpenBox) -> Self {
        Self::from_with_type(device_name, DeviceType::other(), open_box)
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceData, DeviceType};
    use serde_json::json;

    #[test]
    fn device_type_default_is_other() {
        assert_eq!(DeviceType::default().as_str(), "Other");
    }

    #[test]
    fn device_type_named_constructors_are_stable() {
        assert_eq!(DeviceType::android().as_str(), "Android");
        assert_eq!(DeviceType::iphone().as_str(), "iPhone");
        assert_eq!(DeviceType::tablet().as_str(), "Tablet");
        assert_eq!(DeviceType::web().as_str(), "Web");
        assert_eq!(DeviceType::cli().as_str(), "CLI");
        assert_eq!(DeviceType::desktop().as_str(), "Desktop");
    }

    #[test]
    fn device_data_requires_device_type_in_json() {
        let payload = json!({
            "deviceId": "abc",
            "deviceName": "test-device",
            "keys": {
                "dsaPk": "dsa",
                "transportPk": "transport"
            }
        });

        let parsed = serde_json::from_value::<DeviceData>(payload);
        assert!(parsed.is_err());
    }
}
