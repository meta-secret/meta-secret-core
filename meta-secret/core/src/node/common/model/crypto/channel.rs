use crate::CoreResult;
use crate::crypto::keys::TransportPk;
use crate::errors::CoreError;
use anyhow::{Result, bail};
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
    pub fn build(sender: TransportPk, receiver: TransportPk) -> CommunicationChannel {
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
            CommunicationChannel::End2End(End2EndChannel { receiver, .. }) => receiver,
            CommunicationChannel::SingleDevice(LoopbackChannel { device }) => device,
        }
    }

    /// Get a peer/opponent to a given entity
    pub fn peer(&self, initiator_pk: &TransportPk) -> Result<&TransportPk> {
        let sender = self.sender();
        let receiver = self.receiver();

        match initiator_pk {
            pk if pk.eq(sender) => Ok(receiver),
            pk if pk.eq(receiver) => Ok(sender),
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

#[cfg(test)]
mod test {
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::crypto::keys::fixture::KeyManagerFixture;
    use crate::node::common::model::crypto::channel::CommunicationChannel;

    #[test]
    fn test_channel_inverse() {
        let fixture = KeyManagerFixture::generate();
        let alice_km = fixture.client;
        let bob_km = fixture.client_b;

        let sender = alice_km.transport.pk();
        let receiver = bob_km.transport.pk();

        let channel = CommunicationChannel::build(sender.clone(), receiver.clone());
        let inverse_channel = channel.inverse();

        assert_eq!(&receiver, inverse_channel.sender());
        assert_eq!(&sender, inverse_channel.receiver());
    }

    #[test]
    fn test_channel_peer_function_end2end() {
        // Create key managers for testing
        let fixture = KeyManagerFixture::generate();
        let alice_km = fixture.client;
        let bob_km = fixture.client_b;

        let sender_pk = alice_km.transport.pk();
        let receiver_pk = bob_km.transport.pk();

        // Create an End2End channel
        let channel = CommunicationChannel::build(sender_pk.clone(), receiver_pk.clone());

        // Test peer() for sender
        let peer_of_sender = channel.peer(&sender_pk).unwrap();
        assert_eq!(peer_of_sender, &receiver_pk);

        // Test peer() for receiver
        let peer_of_receiver = channel.peer(&receiver_pk).unwrap();
        assert_eq!(peer_of_receiver, &sender_pk);

        // Test peer() with unknown public key
        let charlie_km = KeyManager::generate(); // We still need to generate a separate key manager for Charlie
        let result = channel.peer(&charlie_km.transport.pk());
        assert!(result.is_err());
    }

    #[test]
    fn test_channel_peer_function_loopback() {
        // Create key manager for testing
        let fixture = KeyManagerFixture::generate();
        let device_km = fixture.client; // Using client as the device
        let device_pk = device_km.transport.pk();

        // Create a SingleDevice channel
        let loopback = CommunicationChannel::single_device(device_pk.clone());
        let channel = loopback.to_channel();

        // In a SingleDevice channel, the peer of the device should be itself
        let peer = channel.peer(&device_pk).unwrap();
        assert_eq!(peer, &device_pk);

        // Test peer() with unknown public key
        let other_km = fixture.client_b; // Using client_b as the other device
        let result = channel.peer(&other_km.transport.pk());
        assert!(result.is_err());
    }

    #[test]
    fn test_channel_contains() {
        let fixture = KeyManagerFixture::generate();
        let alice_km = fixture.client;
        let bob_km = fixture.client_b;
        let charlie_km = KeyManager::generate(); // Still need a third key manager

        let sender_pk = alice_km.transport.pk();
        let receiver_pk = bob_km.transport.pk();
        let outsider_pk = charlie_km.transport.pk();

        // Create an End2End channel
        let channel = CommunicationChannel::build(sender_pk.clone(), receiver_pk.clone());

        // The channel should contain both sender and receiver
        assert!(channel.contains(&sender_pk));
        assert!(channel.contains(&receiver_pk));

        // But not an outsider
        assert!(!channel.contains(&outsider_pk));

        // Test for SingleDevice channel
        let loopback = CommunicationChannel::single_device(sender_pk.clone());
        let single_channel = loopback.to_channel();

        assert!(single_channel.contains(&sender_pk));
        assert!(!single_channel.contains(&receiver_pk));
    }

    #[test]
    fn test_loopback_to_end2end_conversion() {
        let fixture = KeyManagerFixture::generate();
        let alice_km = fixture.client;
        let bob_km = fixture.client_b;

        let device_pk = alice_km.transport.pk();
        let receiver_pk = bob_km.transport.pk();

        // Create a SingleDevice channel and convert to End2End
        let loopback = CommunicationChannel::single_device(device_pk.clone());
        let e2e_channel = loopback.build_end_2_end(receiver_pk.clone()).unwrap();

        assert_eq!(e2e_channel.sender, device_pk);
        assert_eq!(e2e_channel.receiver, receiver_pk);

        // Convert to CommunicationChannel
        let channel = e2e_channel.to_channel();

        match channel {
            CommunicationChannel::End2End(e2e) => {
                assert_eq!(e2e.sender, device_pk);
                assert_eq!(e2e.receiver, receiver_pk);
            }
            _ => panic!("Expected End2End channel"),
        }
    }

    #[test]
    fn test_end2end_channel_creation() {
        let fixture = KeyManagerFixture::generate();
        let alice_km = fixture.client;
        let bob_km = fixture.client_b;

        let sender_pk = alice_km.transport.pk();
        let receiver_pk = bob_km.transport.pk();

        // Create an End2End channel using build function
        let channel = CommunicationChannel::build(sender_pk.clone(), receiver_pk.clone());

        // Verify the channel is End2End and has correct properties
        match channel {
            CommunicationChannel::End2End(e2e) => {
                assert_eq!(e2e.sender, sender_pk);
                assert_eq!(e2e.receiver, receiver_pk);
            }
            _ => panic!("Expected End2End channel"),
        }
    }

    #[test]
    fn test_channel_recipients() {
        let fixture = KeyManagerFixture::generate();
        let alice_km = fixture.client;
        let bob_km = fixture.client_b;

        let sender_pk = alice_km.transport.pk();
        let receiver_pk = bob_km.transport.pk();

        // Create an End2End channel
        let channel = CommunicationChannel::build(sender_pk.clone(), receiver_pk.clone());

        // Recipients should include both sender and receiver
        let recipients = channel.recipients().unwrap();

        // Since recipients is a Vec<Box<dyn age::Recipient>>, we can only check the count
        // The order might not be guaranteed as it's created from a HashSet
        assert_eq!(recipients.len(), 2);

        // For a single device channel, there should be just one recipient
        let loopback = CommunicationChannel::single_device(sender_pk.clone());
        let single_channel = loopback.to_channel();

        let single_recipients = single_channel.recipients().unwrap();
        assert_eq!(single_recipients.len(), 1);
    }
}
