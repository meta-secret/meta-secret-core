use crate::crypto::{
    key_pair::{DsaKeyPair, KeyPair, TransportDsaKeyPair},
};
use crate::models::{CommunicationChannel, DeviceInfo, SerializedKeyManager, UserSecurityBox, UserSignature};

pub struct KeyManager {
    pub dsa: DsaKeyPair,
    pub transport_key_pair: TransportDsaKeyPair,
}

/// Key manager can be used only with a single vault name (in the future they will be independent entities)
impl KeyManager {
    pub fn generate() -> KeyManager {
        KeyManager {
            dsa: DsaKeyPair::generate(),
            transport_key_pair: TransportDsaKeyPair::generate(),
        }
    }

    pub fn generate_security_box(vault_name: String) -> UserSecurityBox {
        let key_manager = KeyManager::generate();

        let signature = Box::from(key_manager.dsa.sign(vault_name.clone()));
        UserSecurityBox {
            vault_name,
            signature,
            key_manager: Box::from(SerializedKeyManager::from(&key_manager)),
        }
    }
}

impl UserSecurityBox {
    pub fn get_user_sig(&self, device: &DeviceInfo) -> UserSignature {
        let key_manager: KeyManager = KeyManager::try_from(self.key_manager.as_ref()).unwrap();

        UserSignature {
            vault_name: self.vault_name.clone(),
            device: Box::from(device.clone()),
            public_key: Box::from(key_manager.dsa.public_key()),
            transport_public_key: Box::from(key_manager.transport_key_pair.public_key()),
            signature: Box::from(key_manager.dsa.sign(self.vault_name.clone())),
        }
    }
}

impl CommunicationChannel {
    pub fn inverse(self) -> Self {
        Self {
            sender: self.receiver,
            receiver: self.sender,
        }
    }
}
