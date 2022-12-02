use crate::crypto::encoding::Base64EncodedText;
use crate::crypto::key_pair::{DalekPublicKey, DalekSignature, KeyPair};
use crate::crypto::keys::KeyManager;
use ed25519_dalek::{SignatureError, Verifier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub device_name: String,
    pub device_id: String,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserSignature {
    /// distributed vault, unique across entire system
    pub vault_name: String,
    pub device: DeviceInfo,
    pub public_key: Base64EncodedText,
    pub transport_public_key: Base64EncodedText,

    /// Users' signature. Can be verified by:
    ///     ```signature == ed_dsa::verify(message: user_name, key: public_key)```
    pub signature: Base64EncodedText,
}

impl UserSignature {
    pub fn to_initial_vault_doc(self) -> VaultDoc {
        VaultDoc {
            vault_name: self.vault_name.clone(),
            signatures: vec![self],
            pending_joins: vec![],
            declined_joins: vec![],
        }
    }

    pub fn generate_default_for_tests(key_manager: &KeyManager) -> UserSignature {
        let vault_name = "test_vault".to_string();

        UserSignature {
            vault_name: vault_name.clone(),
            device: DeviceInfo {
                device_name: "test_device".to_string(),
                device_id: "123".to_string(),
            },
            public_key: key_manager.dsa.public_key(),
            transport_public_key: key_manager.transport_key_pair.public_key(),
            signature: key_manager.dsa.sign(vault_name),
        }
    }

    pub fn validate(&self) -> Result<(), SignatureError> {
        let dalek_pk = DalekPublicKey::from(&self.public_key);
        let dalek_signature = DalekSignature::from(&self.signature);

        let msg = self.vault_name.as_bytes();

        dalek_pk.verify(msg, &dalek_signature)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultDoc {
    pub vault_name: String,
    pub signatures: Vec<UserSignature>,
    pub pending_joins: Vec<UserSignature>,
    pub declined_joins: Vec<UserSignature>,
}
