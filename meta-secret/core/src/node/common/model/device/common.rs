use std::fmt::Display;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::crypto::keys::OpenBox;
use crate::crypto::utils::{U64IdUrlEnc, UuidUrlEnc};
use crate::node::common::model::device::device_link::LoopbackDeviceLink;
use crate::node::common::model::IdString;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct DeviceId(pub U64IdUrlEnc);

#[wasm_bindgen]
impl DeviceId {
    pub fn as_str(self) -> String {
        self.0.id_str()
    }
}
impl DeviceId {
    pub fn loopback(self) -> LoopbackDeviceLink {
        LoopbackDeviceLink::from(self)
    }
}

impl Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.clone().as_str())
    }
}

impl From<&OpenBox> for DeviceId {
    fn from(open_box: &OpenBox) -> Self {
        open_box.transport_pk.to_device_id()
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

    pub fn client_b() -> Self {
        DeviceName::from("client_device__b")
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
