use crate::crypto::keys::TransportPk;
use crate::errors::CoreError;
use crate::CoreResult;
use anyhow::{bail, Result};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommunicationChannel {
    End2End(End2EndChannel),
    SingleDevice(LoopbackChannel),
}

impl CommunicationChannel {
    pub fn recipients(&self) -> Result<Vec<Box<dyn age::Recipient>>> {
        let mut recipients: Vec<Box<dyn age::Recipient>> = vec![];

        let pks = HashSet::from([self.sender(), self.receiver()]);
        for pk in pks {
            recipients.push(Box::new(pk.as_recipient()?));
        }

        Ok(recipients)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct End2EndChannel {
    sender: TransportPk,
    receiver: TransportPk,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopbackChannel {
    device: TransportPk,
}

impl LoopbackChannel {
    pub fn build_end_2_end(self, receiver: TransportPk) -> CoreResult<End2EndChannel> {
        if self.device == receiver {
            Err(CoreError::CommunicationChannelError {
                device: self.device,
            })
        } else {
            Ok(End2EndChannel {
                sender: self.device,
                receiver,
            })
        }
    }

    pub fn to_channel(self) -> CommunicationChannel {
        CommunicationChannel::SingleDevice(self)
    }
}

impl End2EndChannel {
    pub fn to_channel(self) -> CommunicationChannel {
        CommunicationChannel::End2End(self)
    }
}

impl CommunicationChannel {
    pub fn end_to_end(sender: TransportPk, receiver: TransportPk) -> CommunicationChannel {
        if sender == receiver {
            CommunicationChannel::SingleDevice(LoopbackChannel { device: sender })
        } else {
            CommunicationChannel::End2End(End2EndChannel { sender, receiver })
        }
    }

    pub fn single_device(device: TransportPk) -> LoopbackChannel {
        LoopbackChannel { device }
    }

    pub fn inverse(self) -> Self {
        match self {
            CommunicationChannel::End2End(End2EndChannel { sender, receiver }) => {
                CommunicationChannel::End2End(End2EndChannel {
                    sender: receiver,
                    receiver: sender,
                })
            }
            CommunicationChannel::SingleDevice { .. } => self,
        }
    }

    pub fn sender(&self) -> &TransportPk {
        match self {
            CommunicationChannel::End2End(End2EndChannel { sender, .. }) => &sender,
            CommunicationChannel::SingleDevice(LoopbackChannel { device }) => &device,
        }
    }

    pub fn receiver(&self) -> &TransportPk {
        match self {
            CommunicationChannel::End2End(End2EndChannel { receiver, .. }) => &receiver,
            CommunicationChannel::SingleDevice(LoopbackChannel { device }) => &device,
        }
    }

    /// Get a peer/opponent to a given entity
    pub fn peer(&self, initiator_pk: &TransportPk) -> Result<&TransportPk> {
        let sender = self.sender();
        let receiver = self.receiver();

        match initiator_pk {
            pk if pk.eq(&sender) => Ok(receiver),
            pk if pk.eq(&receiver) => Ok(sender),
            _ => bail!(CoreError::ThirdPartyEncryptionError {
                key_manager_pk: initiator_pk.clone(),
                channel: self.clone(),
            }),
        }
    }

    pub fn contains(&self, pk: &TransportPk) -> bool {
        match self {
            CommunicationChannel::End2End(channel) => {
                channel.sender.eq(pk) || channel.receiver.eq(pk)
            }
            CommunicationChannel::SingleDevice(channel) => channel.device.eq(pk),
        }
    }
}
