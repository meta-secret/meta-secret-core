use crate::crypto;

use crate::crypto::key_pair::KeyPair;
use crate::crypto::keys::KeyManager;
use crate::models::{DeviceInfo, MetaVault, UserSignature, VaultDoc};

impl UserSignature {
    pub fn to_initial_vault_doc(self) -> VaultDoc {
        VaultDoc {
            vault_name: self.vault.name.clone(),
            signatures: vec![self],
            pending_joins: vec![],
            declined_joins: vec![],
        }
    }

    pub fn generate_default_for_tests(key_manager: &KeyManager) -> Self {
        let vault_name = "test_vault".to_string();

        UserSignature {
            vault: Box::new(MetaVault {
                name: vault_name,
                device: Box::from(DeviceInfo {
                    device_name: "test_device".to_string(),
                    device_id: crypto::utils::rand_uuid_b64_url_enc().base64_text,
                }),
            }),
            public_key: Box::from(key_manager.dsa.public_key()),
            transport_public_key: Box::from(key_manager.transport_key_pair.public_key()),
        }
    }
}
