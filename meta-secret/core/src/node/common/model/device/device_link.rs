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
        let mut ids = [link.sender().id_str(), link.receiver().id_str()];
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
    fn id_str(self) -> String {
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

#[allow(dead_code)]
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
    fn test_device_link_builder() -> Result<()> {
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

    #[test]
    fn test_building_device_link_id_from_device_link() -> Result<()> {
        let sender_km = KeyManager::generate();
        let receiver_km = KeyManager::generate();

        let sender = sender_km.transport.pk().to_device_id();
        let receiver = receiver_km.transport.pk().to_device_id();

        let device_link = sender
            .clone()
            .loopback()
            .peer_to_peer(receiver.clone())?
            .to_device_link();

        let link_id = DeviceLinkId::from(device_link);

        println!("{:?}", link_id.id_str());

        Ok(())
    }

    #[test]
    fn test_is_loopback() -> Result<()> {
        let km = KeyManager::generate();
        let device_id = km.transport.pk().to_device_id();

        let loopback_link = device_id.clone().loopback().to_device_link();
        assert!(loopback_link.is_loopback());

        let km2 = KeyManager::generate();
        let device_id2 = km2.transport.pk().to_device_id();

        let p2p_link = device_id
            .loopback()
            .peer_to_peer(device_id2)?
            .to_device_link();
        assert!(!p2p_link.is_loopback());

        Ok(())
    }

    #[test]
    fn test_device_link_contains() -> Result<()> {
        let km1 = KeyManager::generate();
        let km2 = KeyManager::generate();
        let km3 = KeyManager::generate();

        let device_id1 = km1.transport.pk().to_device_id();
        let device_id2 = km2.transport.pk().to_device_id();
        let device_id3 = km3.transport.pk().to_device_id();

        // Test loopback contains
        let loopback_link = device_id1.clone().loopback().to_device_link();
        assert!(loopback_link.contains(&device_id1));
        assert!(!loopback_link.contains(&device_id2));

        // Test peer-to-peer contains
        let p2p_link = device_id1
            .clone()
            .loopback()
            .peer_to_peer(device_id2.clone())?
            .to_device_link();
        assert!(p2p_link.contains(&device_id1));
        assert!(p2p_link.contains(&device_id2));
        assert!(!p2p_link.contains(&device_id3));

        Ok(())
    }

    #[test]
    fn test_peer_to_peer_inverse() -> Result<()> {
        let km1 = KeyManager::generate();
        let km2 = KeyManager::generate();

        let device_id1 = km1.transport.pk().to_device_id();
        let device_id2 = km2.transport.pk().to_device_id();

        let p2p_link = PeerToPeerDeviceLink {
            sender: device_id1.clone(),
            receiver: device_id2.clone(),
        };

        let inverse = p2p_link.inverse();

        assert_eq!(inverse.sender, device_id2);
        assert_eq!(inverse.receiver, device_id1);

        // Double inverse should equal original
        let double_inverse = inverse.inverse();
        assert_eq!(double_inverse.sender, device_id1);
        assert_eq!(double_inverse.receiver, device_id2);

        Ok(())
    }

    #[test]
    fn test_peer_to_peer_creation_error() -> Result<()> {
        let km = KeyManager::generate();
        let device_id = km.transport.pk().to_device_id();

        // Try to create a peer-to-peer link with the same device ID for both sender and receiver
        let result = device_id.clone().loopback().peer_to_peer(device_id);

        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_device_link_id_properties() -> Result<()> {
        let km1 = KeyManager::generate();
        let km2 = KeyManager::generate();

        let device_id1 = km1.transport.pk().to_device_id();
        let device_id2 = km2.transport.pk().to_device_id();

        // Create a link from device1 to device2
        let forward_link = device_id1
            .clone()
            .loopback()
            .peer_to_peer(device_id2.clone())?
            .to_device_link();

        // Create a link from device2 to device1
        let reverse_link = device_id2
            .loopback()
            .peer_to_peer(device_id1)?
            .to_device_link();

        // The IDs should be the same regardless of the order
        let forward_id = DeviceLinkId::from(forward_link);
        let reverse_id = DeviceLinkId::from(reverse_link);

        assert_eq!(forward_id.id_str(), reverse_id.id_str());

        Ok(())
    }

    #[test]
    fn test_try_from_communication_channel() -> Result<()> {
        use crate::node::common::model::crypto::channel::CommunicationChannel;

        let km1 = KeyManager::generate();
        let km2 = KeyManager::generate();

        let sender_id = km1.transport.pk().to_device_id();
        let receiver_id = km2.transport.pk().to_device_id();

        // Create a communication channel using the transport public keys
        let channel = CommunicationChannel::build(km1.transport.pk(), km2.transport.pk());

        let device_link = DeviceLink::try_from(&channel)?;

        // Verify the sender and receiver are correct
        assert_eq!(device_link.sender(), sender_id);
        assert_eq!(device_link.receiver(), receiver_id);

        // Verify it's a P2P link
        assert!(!device_link.is_loopback());

        Ok(())
    }

    #[test]
    fn test_device_link_sender_receiver() -> Result<()> {
        // Test for loopback link
        let km = KeyManager::generate();
        let device_id = km.transport.pk().to_device_id();

        let loopback_link = device_id.clone().loopback().to_device_link();

        // For loopback links, sender and receiver should be the same device
        assert_eq!(loopback_link.sender(), device_id.clone());
        assert_eq!(loopback_link.receiver(), device_id.clone());

        // Test for peer-to-peer link
        let km1 = KeyManager::generate();
        let km2 = KeyManager::generate();

        let device_id1 = km1.transport.pk().to_device_id();
        let device_id2 = km2.transport.pk().to_device_id();

        let p2p_link = device_id1
            .clone()
            .loopback()
            .peer_to_peer(device_id2.clone())?
            .to_device_link();

        // For P2P links, sender and receiver should be different
        assert_eq!(p2p_link.sender(), device_id1);
        assert_eq!(p2p_link.receiver(), device_id2);

        Ok(())
    }
}
