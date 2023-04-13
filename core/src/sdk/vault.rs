use crate::crypto::key_pair::KeyPair;
use crate::crypto::keys::KeyManager;
use crate::models::{DeviceInfo, UserSignature, VaultDoc};
use rand::Rng;

impl UserSignature {
    pub fn to_initial_vault_doc(self) -> VaultDoc {
        VaultDoc {
            vault_name: self.vault_name.clone(),
            signatures: vec![self],
            pending_joins: vec![],
            declined_joins: vec![],
        }
    }

    pub fn generate_default_for_tests(key_manager: &KeyManager) -> Self {
        let vault_name = "test_vault".to_string();

        let mut rng = rand::thread_rng();

        UserSignature {
            vault_name: vault_name.clone(),
            device: Box::from(DeviceInfo {
                device_name: "test_device".to_string(),
                device_id: rng.gen::<u128>().to_string(),
            }),
            public_key: Box::from(key_manager.dsa.public_key()),
            transport_public_key: Box::from(key_manager.transport_key_pair.public_key()),
        }
    }
}
