use crate::crypto::utils::Id48bit;
use crate::node::common::model::crypto::aead::EncryptedMessage;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::common::model::IdString;
use derive_more::From;
use std::collections::HashMap;
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
pub struct ClaimId(pub Id48bit);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDistributionId {
    pub pass_id: MetaPasswordId,
    pub receiver: DeviceId,
}

impl IdString for SsDistributionId {
    fn id_str(self) -> String {
        [self.receiver.id_str(), self.pass_id.id.id_str()].join("|")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
pub struct SsClaimId {
    pub id: ClaimId,
    pub pass_id: MetaPasswordId,
}

impl IdString for SsClaimId {
    fn id_str(self) -> String {
        [self.id.0.id_str(), self.pass_id.id.id_str()].join("|")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsClaimDbId {
    pub claim_id: SsClaimId,
    pub sender: DeviceId,
    pub distribution_id: SsDistributionId,
}

impl IdString for SsClaimDbId {
    fn id_str(self) -> String {
        [
            self.sender.id_str(),
            self.distribution_id.id_str(), 
            self.claim_id.id_str()
        ].join("|")
    }
}

/// SsDistributionClaim represents a specific distribution of a secret across multiple devices.
///
/// This struct allows to easily represent a claim, and enables distribution logic to operate on it.
/// It is an abstraction that simplifies how secrets are shared between devices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsClaim {
    pub id: ClaimId,
    pub dist_claim_id: SsClaimId,

    pub vault_name: VaultName,
    pub sender: DeviceId,

    pub distribution_type: SecretDistributionType,
    // All receivers of secret shares excluding the sender (the sender already has a share).
    pub receivers: Vec<DeviceId>,
    pub status: SsDistributionCompositeStatus,
}

impl SsClaim {
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

    pub fn claim_db_ids(&self) -> Vec<SsClaimDbId> {
        let mut ids = Vec::with_capacity(self.receivers.len());
        for receiver in self.receivers.iter() {
            ids.push(SsClaimDbId {
                claim_id: self.dist_claim_id.clone(),
                sender: self.sender.clone(),
                distribution_id: SsDistributionId {
                    pass_id: self.dist_claim_id.pass_id.clone(),
                    receiver: receiver.clone(),
                },
            });
        }

        ids
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SsDistributionStatus {
    /// Server is waiting for distributions to arrive, to send them to target devices
    Pending,
    /// The receiver device has received the secret
    Delivered,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDistributionCompositeStatus {
    statuses: HashMap<DeviceId, SsDistributionStatus>,
}

impl SsDistributionCompositeStatus {
    pub fn complete(mut self, device_id: DeviceId) -> Self {
        self.statuses.insert(device_id, SsDistributionStatus::Delivered);
        self
    }

    pub fn status(&self) -> SsDistributionStatus {
        let pending = self
            .statuses
            .values()
            .any(|dist_status| matches!(dist_status, SsDistributionStatus::Pending));

        if pending {
            SsDistributionStatus::Pending
        } else {
            SsDistributionStatus::Delivered
        }
    }
}

impl From<Vec<DeviceId>> for SsDistributionCompositeStatus {
    fn from(devices: Vec<DeviceId>) -> Self {
        let mut statuses = HashMap::new();
        for device_id in devices {
            statuses.insert(device_id, SsDistributionStatus::Pending);
        }

        Self { statuses }
    }
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
    pub claim_id: SsClaimId,
    pub secret_message: EncryptedMessage,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsLogData {
    pub claims: HashMap<ClaimId, SsClaim>,
}

impl SsLogData {
    pub fn complete_for_device(mut self, claim_id: ClaimId, device_id: DeviceId) -> Self {
        let maybe_claim = self.claims.remove(&claim_id);

        if let Some(mut claim) = maybe_claim {
            claim.status = claim.status.complete(device_id);
        }

        self
    }
}

impl SsLogData {
    pub fn new(claim: SsClaim) -> Self {
        let mut claims = HashMap::new();
        claims.insert(claim.id.clone(), claim);
        Self { claims }
    }

    pub fn insert(mut self, claim: SsClaim) -> Self {
        self.claims.insert(claim.id.clone(), claim);
        self
    }
}

#[derive(Clone, Debug, From, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct WasmSsLogData(SsLogData);

#[wasm_bindgen]
#[allow(unused)]
pub struct WasmSsDistributionClaim(SsClaim);
impl WasmSsDistributionClaim {}

impl From<SsClaim> for WasmSsDistributionClaim {
    fn from(claim: SsClaim) -> Self {
        WasmSsDistributionClaim(claim)
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::utils::{Id48bit, U64IdUrlEnc};
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::secret::{ClaimId, SecretDistributionType, SsClaim, SsClaimId, SsDistributionCompositeStatus, SsDistributionStatus};
    use crate::node::common::model::vault::vault::VaultName;
    use anyhow::Result;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_distribution_ids() -> Result<()> {
        let registry = FixtureRegistry::empty();

        let client_device_id = registry.state.device_creds.client.device.device_id;
        let client_b_device_id = registry.state.device_creds.client_b.device.device_id;
        let vd_device_id = registry.state.device_creds.vd.device.device_id;

        let mut status = HashMap::new();
        status.insert(vd_device_id.clone(), SsDistributionStatus::Pending);
        status.insert(client_b_device_id.clone(), SsDistributionStatus::Pending);

        let claim_id = ClaimId::from(Id48bit::generate());
        let receivers = vec![vd_device_id, client_b_device_id];

        let claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id,
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("pass_id".to_string()),
                    name: "test_pass".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender: client_device_id,
            distribution_type: SecretDistributionType::Split,
            receivers: receivers.clone(),
            status: SsDistributionCompositeStatus::from(receivers),
        };

        let dist_ids = claim.distribution_ids();

        assert_eq!(2, dist_ids.len());

        Ok(())
    }
}
