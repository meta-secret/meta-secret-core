use crate::crypto::key_pair::{DalekPublicKey, DalekSignature, KeyPair};
use crate::crypto::keys::KeyManager;
use crate::CoreResult;
use ed25519_dalek::Verifier;
use crate::models::{DeviceInfo, UserSignature, VaultDoc};

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

        UserSignature {
            vault_name: vault_name.clone(),
            device: Box::from(DeviceInfo {
                device_name: "test_device".to_string(),
                device_id: "123".to_string(),
            }),
            public_key: Box::from(key_manager.dsa.public_key()),
            transport_public_key: Box::from(key_manager.transport_key_pair.public_key()),
            signature: Box::from(key_manager.dsa.sign(vault_name)),
        }
    }

    pub fn validate(&self) -> CoreResult<()> {
        let pk = DalekPublicKey::try_from(self.public_key.as_ref())?;
        let signature = DalekSignature::try_from(self.signature.as_ref())?;

        let msg = self.vault_name.as_bytes();

        pk.verify(msg, &signature)?;
        Ok(())
    }
}

