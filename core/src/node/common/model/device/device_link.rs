use crate::node::common::model::device::common::DeviceId;
use anyhow::anyhow;
use wasm_bindgen::prelude::wasm_bindgen;
use crate::crypto;
use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::utils::UuidUrlEnc;
use crate::node::common::model::crypto::CommunicationChannel;
use crate::node::common::model::IdString;

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
        
        let id = Base64Text([first_prefix, second_prefix, uuid_str].join("|"));

        Self(id)
    }
}

impl IdString for DeviceLinkId {
    fn id_str(&self) -> String {
        self.0.0.clone()
    }
}

impl DeviceLink {
    pub fn id(&self) -> DeviceLinkId {
        DeviceLinkId::from(self.clone())
    }

    pub fn sender(&self) -> DeviceId {
        match self {
            DeviceLink::Loopback(link) => link.sender().clone(),
            DeviceLink::PeerToPeer(link) => link.sender.clone()
        }
    }

    pub fn receiver(&self) -> DeviceId {
        match self {
            DeviceLink::Loopback(link) => link.receiver().clone(),
            DeviceLink::PeerToPeer(link) => link.receiver().clone()
        }
    }

    pub fn contains(&self, device_id: &DeviceId) -> bool {
        match self {
            DeviceLink::Loopback(link) => {
                link.device.eq(device_id)
            },
            DeviceLink::PeerToPeer(link) => {
                link.receiver.eq(device_id) || link.sender.eq(device_id)
            }
        }
    }
}

impl TryFrom<&CommunicationChannel> for DeviceLink {
    type Error = anyhow::Error;

    fn try_from(channel: &CommunicationChannel) -> Result<Self, Self::Error> {
        DeviceLinkBuilder::builder()
            .sender(channel.sender.to_device_id())
            .receiver(channel.receiver.to_device_id())
            .build()
    }
}

pub struct WasmDeviceLink(DeviceLink);
impl WasmDeviceLink {}

impl From<DeviceLink> for WasmDeviceLink {
    fn from(device_link: DeviceLink) -> Self {
        Self(device_link)
    }
}

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
    pub fn sender(&self) -> &DeviceId {
        &self.device
    }

    pub fn receiver(&self) -> &DeviceId {
        &self.device
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct PeerToPeerDeviceLink {
    sender: DeviceId,
    receiver: DeviceId,
}

impl PeerToPeerDeviceLink {
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

pub struct PeerToPeerDeviceLinkBuilder {
    sender: Option<DeviceId>,
    receiver: Option<DeviceId>,
}

impl PeerToPeerDeviceLinkBuilder {
    pub fn builder() -> Self {
        Self {
            sender: None,
            receiver: None,
        }
    }

    pub fn sender(mut self, sender: DeviceId) -> Self {
        self.sender = Some(sender);
        self
    }

    pub fn receiver(mut self, receiver: DeviceId) -> Self {
        self.receiver = Some(receiver);
        self
    }

    pub fn build(self) -> anyhow::Result<PeerToPeerDeviceLink> {
        let sender = self.sender.ok_or(anyhow!("Sender is not set"))?;
        let receiver = self.receiver.ok_or(anyhow!("Receiver is not set"))?;

        if sender == receiver {
            return Err(anyhow!("Sender and receiver are the same"));
        }

        Ok(PeerToPeerDeviceLink { sender, receiver })
    }
}

pub struct DeviceLinkBuilder {
    sender: Option<DeviceId>,
    receiver: Option<DeviceId>,
}

impl DeviceLinkBuilder {
    pub fn builder() -> Self {
        Self {
            sender: None,
            receiver: None,
        }
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
                    let peer_to_peer = PeerToPeerDeviceLinkBuilder::builder()
                        .sender(sender)
                        .receiver(receiver)
                        .build()?;

                    DeviceLink::PeerToPeer(peer_to_peer)
                }
            }
            None => DeviceLink::Loopback(LoopbackDeviceLink { device: sender }),
        };

        Ok(device_link)
    }
}

impl DeviceLink {
    pub fn is_loopback(&self) -> bool {
        matches!(self, DeviceLink::Loopback(_))
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::{KeyManager, TransportPk};
    use super::*;

    #[test]
    fn test_device_link_builder() -> anyhow::Result<()> {
        let sender_km = KeyManager::generate();
        let receiver_km = KeyManager::generate();
        
        let sender = sender_km.transport.pk().to_device_id();
        let receiver = receiver_km.transport.pk().to_device_id();

        let device_link = DeviceLinkBuilder::builder()
            .sender(sender.clone())
            .receiver(receiver.clone())
            .build()?;

        assert_eq!(
            device_link,
            DeviceLink::PeerToPeer(PeerToPeerDeviceLink {
                sender: sender.clone(),
                receiver
            })
        );

        let device_link = DeviceLinkBuilder::builder()
            .sender(sender.clone())
            .build()?;

        assert_eq!(
            device_link,
            DeviceLink::Loopback(LoopbackDeviceLink { device: sender })
        );

        Ok(())
    }
}
