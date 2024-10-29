use crate::node::common::model::device::common::DeviceId;
use anyhow::anyhow;

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

impl LoopbackDeviceLink {
    pub fn device(&self) -> &DeviceId {
        &self.device
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    use super::*;

    #[test]
    fn test_device_link_builder() -> anyhow::Result<()> {
        let sender = DeviceId::from("sender");
        let receiver = DeviceId::from("receiver");

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

        let device_link = DeviceLinkBuilder::builder().sender(sender.clone()).build()?;

        assert_eq!(device_link, DeviceLink::Loopback(LoopbackDeviceLink { device: sender }));

        Ok(())
    }
}
