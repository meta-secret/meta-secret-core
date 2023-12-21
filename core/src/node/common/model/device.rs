use std::fmt::Display;

use anyhow::anyhow;

use crypto::utils::generate_uuid_b64_url_enc;

use crate::crypto;
use crate::crypto::keys::{KeyManager, OpenBox};
use crate::crypto::keys::SecretBox;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceId(String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceName(String);

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceLink {
    Loopback(LoopbackDeviceLink),
    PeerToPeer(PeerToPeerDeviceLink),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopbackDeviceLink {
    device: DeviceId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerToPeerDeviceLink {
    sender: DeviceId,
    receiver: DeviceId,
}

pub struct DeviceLinkBuilder {
    sender: Option<DeviceId>,
    receiver: Option<DeviceId>,
}

impl DeviceLinkBuilder {
    pub fn new() -> Self {
        Self { sender: None, receiver: None }
    }

    pub fn sender(mut self, sender: DeviceId) -> Self {
        self.sender = Some(sender);
        self
    }

    pub fn receiver(mut self, receiver: DeviceId) -> Self {
        self.receiver = Some(receiver);
        self
    }

    pub fn build(self) -> anyhow::Result<DeviceLink> {
        let sender = self.sender.ok_or(anyhow!("Sender is not set"))?;

        let device_link = match self.receiver {
            Some(receiver) => {
                if sender == receiver {
                    DeviceLink::Loopback(LoopbackDeviceLink { device: sender })
                } else {
                    DeviceLink::PeerToPeer(PeerToPeerDeviceLink { sender, receiver })
                }
            }
            None => {
                DeviceLink::Loopback(LoopbackDeviceLink { device: sender })
            }
        };

        Ok(device_link)
    }
}

impl DeviceLink {
    pub fn is_loopback(&self) -> bool {
        matches!(self, DeviceLink::Loopback(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceData {
    pub id: DeviceId,
    pub name: DeviceName,
    pub keys: OpenBox,
}

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

impl From<&OpenBox> for DeviceId {
    fn from(open_box: &OpenBox) -> Self {
        let dsa_pk = open_box.dsa_pk.base64_text.clone();
        let id = generate_uuid_b64_url_enc(dsa_pk);
        Self(id)
    }
}