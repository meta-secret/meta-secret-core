use std::collections::HashMap;
use derive_more::From;
use crate::crypto::utils::Id48bit;
use crate::node::common::model::crypto::aead::EncryptedMessage;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::{MetaPasswordId, SALT_LENGTH};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::common::model::IdString;
use rand::distributions::Alphanumeric;
use rand::Rng;
use wasm_bindgen::prelude::wasm_bindgen;

/// `ClaimId` is a wrapper around a `String` that serves as a unique identifier
/// for claims within the system. It is used to track and manage claims associated
/// with secret distributions, ensuring each claim can be uniquely identified and
/// referenced. The `ClaimId` is derived from various attributes and is utilized
/// throughout the secret management process to maintain integrity and traceability.
#[derive(Clone, Debug, From, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(transparent)]
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

/// SsDistributionClaim represents a specific distribution of a secret across multiple devices.
///
/// This struct allows to easily represent a claim, and enables distribution logic to operate on it.
/// It is an abstraction that simplifies how secrets are shared between devices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDistributionClaim {
    pub id: Id48bit,
    pub dist_claim_id: SsDistributionClaimId,

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
                pass_id: self.dist_claim_id.pass_id.clone(),
                receiver: receiver.clone(),
            });
        }

        ids
    }

    pub fn claim_db_ids(&self) -> Vec<SsDistributionClaimDbId> {
        let mut ids = Vec::with_capacity(self.receivers.len());
        for receiver in self.receivers.iter() {
            ids.push(SsDistributionClaimDbId {
                claim_id: self.dist_claim_id.clone(),
                distribution_id: SsDistributionId {
                    pass_id: self.dist_claim_id.pass_id.clone(),
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
    pub claims: HashMap<Id48bit, SsDistributionClaim>,
}

impl SsLogData {
    pub fn new(claim: SsDistributionClaim) -> Self {
        let mut claims = HashMap::new();
        claims.insert(claim.id.clone(), claim);
        Self { claims }
    }

    pub fn insert(mut self, claim: SsDistributionClaim) -> Self {
        self.claims.insert(claim.id.clone(), claim);
        self
    }
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
#[allow(unused)]
pub struct WasmSsDistributionClaim(SsDistributionClaim);
impl WasmSsDistributionClaim {}

impl From<SsDistributionClaim> for WasmSsDistributionClaim {
    fn from(claim: SsDistributionClaim) -> Self {
        WasmSsDistributionClaim(claim)
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::utils::{Id48bit, U64IdUrlEnc};
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::secret::{
        ClaimId, SecretDistributionType, SsDistributionClaim, SsDistributionClaimId
    };
    use crate::node::common::model::vault::vault::VaultName;
    use anyhow::Result;

    #[tokio::test]
    async fn test_distribution_ids() -> Result<()> {
        let registry = FixtureRegistry::empty();
        
        let client_device_id = registry.state.device_creds.client.device.device_id;
        let client_b_device_id = registry.state.device_creds.client_b.device.device_id;
        let vd_device_id = registry.state.device_creds.vd.device.device_id;
        
        let claim = SsDistributionClaim {
            id: Id48bit::generate(),
            dist_claim_id: SsDistributionClaimId {
                id: ClaimId::from("123".to_string()),
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("pass_id".to_string()),
                    name: "test_pass".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender: client_device_id,
            distribution_type: SecretDistributionType::Split,
            receivers: vec![vd_device_id, client_b_device_id],
        };

        let dist_ids = claim.distribution_ids();
        
        assert_eq!(2, dist_ids.len());
        
        Ok(())
    }
}