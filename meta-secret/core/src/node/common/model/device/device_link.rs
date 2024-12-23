use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::utils::UuidUrlEnc;
use crate::node::common::model::crypto::channel::CommunicationChannel;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::IdString;
use anyhow::{bail, Result};
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceLink {
    Loopback(LoopbackDeviceLink),
    PeerToPeer(PeerToPeerDeviceLink),
}

pub struct DeviceLinkId(Base64Text);

impl From<DeviceLink> for DeviceLinkId {
    fn from(link: DeviceLink) -> Self {
        let mut ids = vec![link.sender().as_str(), link.receiver().as_str()];
        ids.sort();

        let ids_str = ids.join("");
        let uuid_str = UuidUrlEnc::from(ids_str).id_str();

        let first_prefix: String = ids[0].chars().take(2).collect();
        let second_prefix: String = ids[1].chars().take(2).collect();

        let id = Base64Text::from([first_prefix, second_prefix, uuid_str].join("|"));

        Self(id)
    }
}

impl IdString for DeviceLinkId {
    fn id_str(&self) -> String {
        String::try_from(&self.0).unwrap()
    }
}

impl DeviceLink {
    pub fn id(&self) -> DeviceLinkId {
        DeviceLinkId::from(self.clone())
    }

    pub fn sender(&self) -> DeviceId {
        match self {
            DeviceLink::Loopback(link) => link.sender().clone(),
            DeviceLink::PeerToPeer(link) => link.sender.clone(),
        }
    }

    pub fn receiver(&self) -> DeviceId {
        match self {
            DeviceLink::Loopback(link) => link.receiver().clone(),
            DeviceLink::PeerToPeer(link) => link.receiver().clone(),
        }
    }

    pub fn contains(&self, device_id: &DeviceId) -> bool {
        match self {
            DeviceLink::Loopback(link) => link.device.eq(device_id),
            DeviceLink::PeerToPeer(link) => {
                link.receiver.eq(device_id) || link.sender.eq(device_id)
            }
        }
    }
}

impl TryFrom<&CommunicationChannel> for DeviceLink {
    type Error = anyhow::Error;

    fn try_from(channel: &CommunicationChannel) -> Result<Self, Self::Error> {
        let link = channel
            .sender()
            .to_device_id()
            .loopback()
            .peer_to_peer(channel.receiver().to_device_id())?
            .to_device_link();
        Ok(link)
    }
}

pub struct WasmDeviceLink(DeviceLink);
impl WasmDeviceLink {}

impl From<DeviceLink> for WasmDeviceLink {
    fn from(device_link: DeviceLink) -> Self {
        Self(device_link)
    }
}

/// a <-> a
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct LoopbackDeviceLink {
    device: DeviceId,
}

impl From<DeviceId> for LoopbackDeviceLink {
    fn from(device: DeviceId) -> Self {
        Self { device }
    }
}

impl LoopbackDeviceLink {
    pub fn peer_to_peer(self, receiver: DeviceId) -> Result<PeerToPeerDeviceLink> {
        if self.device == receiver {
            bail!("Sender and receiver are the same");
        }

        Ok(PeerToPeerDeviceLink {
            sender: self.device,
            receiver,
        })
    }

    pub fn to_device_link(self) -> DeviceLink {
        DeviceLink::Loopback(self)
    }

    pub fn sender(&self) -> &DeviceId {
        &self.device
    }

    pub fn receiver(&self) -> &DeviceId {
        &self.device
    }
}

/// a <-> b
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct PeerToPeerDeviceLink {
    sender: DeviceId,
    receiver: DeviceId,
}

impl PeerToPeerDeviceLink {
    pub fn to_device_link(self) -> DeviceLink {
        DeviceLink::PeerToPeer(self)
    }

    pub fn sender(&self) -> &DeviceId {
        &self.sender
    }
    pub fn receiver(&self) -> &DeviceId {
        &self.receiver
    }

    pub fn inverse(&self) -> PeerToPeerDeviceLink {
        PeerToPeerDeviceLink {
            sender: self.receiver.clone(),
            receiver: self.sender.clone(),
        }
    }
}

impl DeviceLink {
    pub fn is_loopback(&self) -> bool {
        matches!(self, DeviceLink::Loopback(_))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;

    #[test]
    fn test_device_link_builder() -> anyhow::Result<()> {
        let sender_km = KeyManager::generate();
        let receiver_km = KeyManager::generate();

        let sender = sender_km.transport.pk().to_device_id();
        let receiver = receiver_km.transport.pk().to_device_id();

        let device_link = sender
            .clone()
            .loopback()
            .peer_to_peer(receiver.clone())?
            .to_device_link();

        assert_eq!(
            device_link,
            DeviceLink::PeerToPeer(PeerToPeerDeviceLink {
                sender: sender.clone(),
                receiver
            })
        );

        let device_link = sender.clone().loopback();

        assert_eq!(device_link, LoopbackDeviceLink { device: sender });

        Ok(())
    }
}
