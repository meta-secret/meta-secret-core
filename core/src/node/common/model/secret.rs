use std::collections::HashMap;

use crate::crypto::utils;
use crate::node::common::model::crypto::EncryptedMessage;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::device::device_link::DeviceLink;
use crate::node::common::model::vault::VaultName;
use rand::distributions::Alphanumeric;
use rand::Rng;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct MetaPasswordId {
    /// SHA256 hash of a salt
    pub id: String,
    /// Random String up to 30 characters, must be unique
    pub salt: String,
    /// Human-readable name given to the password
    pub name: String,
}

const SALT_LENGTH: usize = 8;

impl MetaPasswordId {
    pub fn generate(name: String) -> Self {
        let salt: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(SALT_LENGTH)
            .map(char::from)
            .collect();
        MetaPasswordId::build(name, salt)
    }

    pub fn build(name: String, salt: String) -> Self {
        let mut id_str = name.clone();
        id_str.push('-');
        id_str.push_str(salt.as_str());

        Self {
            id: utils::generate_uuid_b64_url_enc(id_str),
            salt,
            name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDistributionId {
    pub claim_id: SsDistributionClaimId,
    pub distribution_type: SecretDistributionType,
    pub device_link: DeviceLink,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsLogData {
    pub claims: HashMap<SsDistributionClaimId, SsDistributionClaim>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct WasmSsLogData(SsLogData);

impl From<SsLogData> for WasmSsLogData {
    fn from(log: SsLogData) -> Self {
        WasmSsLogData(log)
    }
}

impl SsLogData {
    pub fn empty() -> Self {
        Self { claims: HashMap::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDistributionClaim {
    pub vault_name: VaultName,
    pub owner: DeviceId,

    pub pass_id: MetaPasswordId,

    pub id: SsDistributionClaimId,
    pub distribution_type: SecretDistributionType,

    pub distributions: Vec<DeviceLink>,
}

#[wasm_bindgen]
pub struct WasmSsDistributionClaim(SsDistributionClaim);
impl WasmSsDistributionClaim {
    
}

impl From<SsDistributionClaim> for WasmSsDistributionClaim {
    fn from(claim: SsDistributionClaim) -> Self {
        WasmSsDistributionClaim(claim)
    }
}

impl SsDistributionClaim {
    pub fn distribution_ids(&self) -> Vec<SsDistributionId> {
        let mut ids = Vec::with_capacity(self.distributions.len());
        for device_link in self.distributions.iter() {
            ids.push(SsDistributionId {
                claim_id: self.id.clone(),
                distribution_type: self.distribution_type,
                device_link: device_link.clone(),
            });
        }

        ids
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct SsDistributionClaimId(pub String);

impl SsDistributionClaimId {
    pub fn generate() -> Self {
        let id: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(SALT_LENGTH)
            .map(char::from)
            .collect();
        Self(id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SsDistributionStatus {
    Pending,
    /// The sender device has sent the secret share
    Sent,
    /// The receiver device has received the secret
    Delivered,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub enum SecretDistributionType {
    Split,
    Recover,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretDistributionData {
    pub vault_name: VaultName,
    pub pass_id: MetaPasswordId,
    pub secret_message: EncryptedMessage,
}
