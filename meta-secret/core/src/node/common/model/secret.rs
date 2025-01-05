use std::collections::HashMap;

use crate::node::common::model::crypto::aead::EncryptedMessage;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::{MetaPasswordId, SALT_LENGTH};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::common::model::IdString;
use rand::distributions::Alphanumeric;
use rand::Rng;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct ClaimId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDistributionId {
    pub pass_id: MetaPasswordId,
    pub receiver: DeviceId,
}

impl IdString for SsDistributionId {
    fn id_str(self) -> String {
        [self.receiver.as_str(), self.pass_id.id.id_str()].join("|")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct SsDistributionClaimId {
    pub id: ClaimId,
    pub pass_id: MetaPasswordId,
}

impl IdString for SsDistributionClaimId {
    fn id_str(self) -> String {
        [self.id.0.clone(), self.pass_id.id.id_str()].join("|")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDistributionClaimDbId {
    pub claim_id: SsDistributionClaimId,
    pub distribution_id: SsDistributionId,
}

impl IdString for SsDistributionClaimDbId {
    fn id_str(self) -> String {
        [self.distribution_id.id_str(), self.claim_id.id_str()].join("|")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDistributionClaim {
    pub id: SsDistributionClaimId,

    pub vault_name: VaultName,
    pub sender: DeviceId,

    pub distribution_type: SecretDistributionType,
    pub receivers: Vec<DeviceId>,
}

impl SsDistributionClaim {
    pub fn distribution_ids(&self) -> Vec<SsDistributionId> {
        let mut ids = Vec::with_capacity(self.receivers.len());
        for receiver in self.receivers.iter() {
            ids.push(SsDistributionId {
                pass_id: self.id.pass_id.clone(),
                receiver: receiver.clone(),
            });
        }

        ids
    }

    pub fn claim_db_ids(&self) -> Vec<SsDistributionClaimDbId> {
        let mut ids = Vec::with_capacity(self.receivers.len());
        for receiver in self.receivers.iter() {
            ids.push(SsDistributionClaimDbId {
                claim_id: self.id.clone(),
                distribution_id: SsDistributionId {
                    pass_id: self.id.pass_id.clone(),
                    receiver: receiver.clone(),
                },
            });
        }

        ids
    }
}

impl From<MetaPasswordId> for SsDistributionClaimId {
    fn from(pass_id: MetaPasswordId) -> Self {
        let id: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(SALT_LENGTH)
            .map(char::from)
            .collect();
        Self {
            id: ClaimId(id),
            pass_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SsDistributionStatus {
    /// Server is waiting for distributions to arrive, to send them to target devices
    Pending,
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
    pub claim_id: SsDistributionClaimId,
    pub secret_message: EncryptedMessage,
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
        Self {
            claims: HashMap::new(),
        }
    }
}

#[wasm_bindgen]
pub struct WasmSsDistributionClaim(SsDistributionClaim);
impl WasmSsDistributionClaim {}

impl From<SsDistributionClaim> for WasmSsDistributionClaim {
    fn from(claim: SsDistributionClaim) -> Self {
        WasmSsDistributionClaim(claim)
    }
}
