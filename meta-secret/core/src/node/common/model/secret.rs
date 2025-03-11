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
pub struct SsRecoveryId {
    pub claim_id: SsClaimId,
    pub sender: DeviceId,
    pub distribution_id: SsDistributionId,
}

impl IdString for SsRecoveryId {
    fn id_str(self) -> String {
        [
            self.sender.id_str(),
            self.distribution_id.id_str(),
            self.claim_id.id_str(),
        ]
        .join("|")
    }
}

/// SsDistributionClaim represents a specific distribution of a secret across multiple devices.
///
/// This struct allows to easily represent a claim, and enables distribution logic to operate on it.
/// It is an abstraction that simplifies how secrets are shared between devices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsClaim {
    /// The unique identifier (in the hashmap) for the claim
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

    pub fn recovery_db_ids(&self) -> Vec<SsRecoveryId> {
        let mut ids = Vec::with_capacity(self.receivers.len());
        for receiver in self.receivers.iter() {
            ids.push(SsRecoveryId {
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
    /// The distribution is on the server
    Sent,
    /// The receiver device has received the secret
    Delivered,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDistributionCompositeStatus {
    pub statuses: HashMap<DeviceId, SsDistributionStatus>,
}

impl SsDistributionCompositeStatus {
    pub fn sent(mut self, device_id: DeviceId) -> Self {
        self.statuses.insert(device_id, SsDistributionStatus::Sent);
        self
    }

    pub fn complete(mut self, device_id: DeviceId) -> Self {
        self.statuses
            .insert(device_id, SsDistributionStatus::Delivered);
        self
    }

    pub fn get(&self, device_id: &DeviceId) -> Option<&SsDistributionStatus> {
        self.statuses.get(device_id)
    }

    pub fn status(&self) -> SsDistributionStatus {
        let pending = self
            .statuses
            .values()
            .any(|dist_status| matches!(dist_status, SsDistributionStatus::Pending));

        let delivered = self
            .statuses
            .values()
            .all(|dist_status| matches!(dist_status, SsDistributionStatus::Delivered));

        if pending {
            SsDistributionStatus::Pending
        } else if delivered {
            SsDistributionStatus::Delivered
        } else {
            SsDistributionStatus::Sent
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
    pub fn sent(mut self, claim_id: ClaimId, device_id: DeviceId) -> Self {
        let maybe_claim = self.claims.remove(&claim_id);

        if let Some(mut claim) = maybe_claim {
            claim.status = claim.status.sent(device_id);
            // Insert the updated claim back into the hashmap
            self.claims.insert(claim_id, claim);
        }

        self
    }

    pub fn complete(mut self, claim_id: ClaimId, device_id: DeviceId) -> Self {
        let maybe_claim = self.claims.remove(&claim_id);

        if let Some(mut claim) = maybe_claim {
            claim.status = claim.status.complete(device_id);
            
            if  claim.status.status() != SsDistributionStatus::Delivered {
                // Insert the updated claim back into the hashmap
                self.claims.insert(claim_id, claim);
            }
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
    use crate::node::common::model::device::common::DeviceId;
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::secret::{
        ClaimId, SecretDistributionType, SsClaim, SsClaimId, SsDistributionCompositeStatus,
        SsDistributionStatus, SsLogData,
    };
    use crate::node::common::model::vault::vault::VaultName;
    use anyhow::Result;

    #[tokio::test]
    async fn test_distribution_ids() -> Result<()> {
        let registry = FixtureRegistry::empty();

        let client_device_id = registry.state.device_creds.client.device.device_id;
        let client_b_device_id = registry.state.device_creds.client_b.device.device_id;
        let vd_device_id = registry.state.device_creds.vd.device.device_id;

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

    #[tokio::test]
    async fn test_recovery_db_ids() -> Result<()> {
        // Setup with fixtures
        let registry = FixtureRegistry::empty();

        let client_device_id = registry.state.device_creds.client.device.device_id;
        let client_b_device_id = registry.state.device_creds.client_b.device.device_id;
        let vd_device_id = registry.state.device_creds.vd.device.device_id;

        // Create a claim
        let claim_id = ClaimId::from(Id48bit::generate());
        let pass_id = MetaPasswordId {
            id: U64IdUrlEnc::from("test_pass_id".to_string()),
            name: "test_pass".to_string(),
        };
        let ss_claim_id = SsClaimId {
            id: claim_id.clone(),
            pass_id: pass_id.clone(),
        };
        let receivers = vec![client_b_device_id.clone(), vd_device_id.clone()];

        let claim = SsClaim {
            id: claim_id,
            dist_claim_id: ss_claim_id.clone(),
            vault_name: VaultName::test(),
            sender: client_device_id.clone(),
            distribution_type: SecretDistributionType::Split,
            receivers: receivers.clone(),
            status: SsDistributionCompositeStatus::from(receivers.clone()),
        };

        // Generate recovery IDs
        let recovery_ids = claim.recovery_db_ids();

        // Verify IDs are constructed correctly
        assert_eq!(
            recovery_ids.len(),
            2,
            "Should generate recovery IDs for both receivers"
        );

        // Check first recovery ID
        let recovery_id_0 = &recovery_ids[0];
        assert_eq!(
            recovery_id_0.claim_id.id, claim.dist_claim_id.id,
            "Claim ID should match"
        );
        assert_eq!(
            recovery_id_0.claim_id.pass_id.id, pass_id.id,
            "Password ID should match"
        );
        assert_eq!(
            recovery_id_0.sender, client_device_id,
            "Sender should match"
        );
        assert_eq!(
            recovery_id_0.distribution_id.pass_id.id, pass_id.id,
            "Distribution pass ID should match"
        );
        assert_eq!(
            recovery_id_0.distribution_id.receiver, receivers[0],
            "Receiver should match first receiver"
        );

        // Check second recovery ID
        let recovery_id_1 = &recovery_ids[1];
        assert_eq!(
            recovery_id_1.claim_id.id, claim.dist_claim_id.id,
            "Claim ID should match"
        );
        assert_eq!(
            recovery_id_1.claim_id.pass_id.id, pass_id.id,
            "Password ID should match"
        );
        assert_eq!(
            recovery_id_1.sender, client_device_id,
            "Sender should match"
        );
        assert_eq!(
            recovery_id_1.distribution_id.pass_id.id, pass_id.id,
            "Distribution pass ID should match"
        );
        assert_eq!(
            recovery_id_1.distribution_id.receiver, receivers[1],
            "Receiver should match second receiver"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_ss_distribution_composite_status() -> Result<()> {
        // Setup with fixtures
        let registry = FixtureRegistry::empty();
        let client_b_device_id = registry.state.device_creds.client_b.device.device_id;
        let vd_device_id = registry.state.device_creds.vd.device.device_id;

        // Test creation from Vec<DeviceId>
        let devices = vec![client_b_device_id.clone(), vd_device_id.clone()];
        let status = SsDistributionCompositeStatus::from(devices);

        // Verify all device statuses are pending
        assert_eq!(
            status.statuses.len(),
            2,
            "Should have status for all two devices"
        );

        for (_, device_status) in &status.statuses {
            assert!(
                matches!(device_status, SsDistributionStatus::Pending),
                "Initial status should be Pending"
            );
        }

        // Test sent method
        let updated_status = status.sent(client_b_device_id.clone());
        assert!(
            matches!(
                updated_status.get(&client_b_device_id),
                Some(SsDistributionStatus::Sent)
            ),
            "Status for client b should be Sent"
        );
        assert!(
            matches!(
                updated_status.get(&vd_device_id),
                Some(SsDistributionStatus::Pending)
            ),
            "Status for vd device should still be Pending"
        );

        // Test complete method
        let final_status = updated_status.complete(client_b_device_id.clone());
        assert!(
            matches!(
                final_status.get(&client_b_device_id),
                Some(SsDistributionStatus::Delivered)
            ),
            "Status for device_1 should be Delivered"
        );
        assert!(
            matches!(
                final_status.get(&vd_device_id),
                Some(SsDistributionStatus::Pending)
            ),
            "Status for vd_device_id should still be Pending"
        );

        // Test non-existent device
        let non_existent_device = DeviceId(U64IdUrlEnc::from("non_existent_device".to_string()));
        assert_eq!(
            final_status.get(&non_existent_device),
            None,
            "Non-existent device should return None"
        );

        // Test overall status method
        let all_pending = SsDistributionCompositeStatus::from(vec![
            client_b_device_id.clone(),
            vd_device_id.clone(),
        ]);
        assert!(
            matches!(all_pending.status(), SsDistributionStatus::Pending),
            "Overall status should be Pending when all are Pending"
        );

        let mut mixed_statuses = all_pending.clone();
        mixed_statuses = mixed_statuses.sent(client_b_device_id.clone());
        assert!(
            matches!(mixed_statuses.status(), SsDistributionStatus::Pending),
            "Overall status should be Pending when at least one is Pending"
        );

        let mut all_sent = mixed_statuses.clone();
        all_sent = all_sent.sent(vd_device_id.clone());
        assert!(
            matches!(all_sent.status(), SsDistributionStatus::Sent),
            "Overall status should be Sent when all are at least Sent"
        );

        let mut partially_delivered = all_sent.clone();
        partially_delivered = partially_delivered.complete(client_b_device_id.clone());
        assert!(
            matches!(partially_delivered.status(), SsDistributionStatus::Sent),
            "Overall status should be Sent when not all are Delivered"
        );

        let all_delivered = partially_delivered.complete(vd_device_id.clone());
        assert!(
            matches!(all_delivered.status(), SsDistributionStatus::Delivered),
            "Overall status should be Delivered when all are Delivered"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_ss_log_data_sent_and_complete() -> Result<()> {
        // Setup with fixtures
        let registry = FixtureRegistry::empty();

        let client_device_id = registry.state.device_creds.client.device.device_id;
        let client_b_device_id = registry.state.device_creds.client_b.device.device_id;
        let vd_device_id = registry.state.device_creds.vd.device.device_id;

        // Create a claim
        let claim_id = ClaimId::from(Id48bit::generate());
        let receivers = vec![client_b_device_id.clone(), vd_device_id.clone()];

        let claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id.clone(),
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("pass_id".to_string()),
                    name: "test_pass".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender: client_device_id.clone(),
            distribution_type: SecretDistributionType::Split,
            receivers: receivers.clone(),
            status: SsDistributionCompositeStatus::from(receivers.clone()),
        };

        // Create log data with the claim
        let log_data = SsLogData::new(claim.clone());

        // Verify the claim is in the log data
        assert_eq!(
            log_data.claims.len(),
            1,
            "Log data should contain one claim"
        );
        assert!(
            log_data.claims.contains_key(&claim_id),
            "Log data should contain our claim"
        );

        let stored_claim = log_data.claims.get(&claim_id).unwrap();
        // Check if all statuses are Pending initially
        for receiver in &receivers {
            let status = stored_claim.status.get(receiver).unwrap();
            assert!(
                matches!(status, SsDistributionStatus::Pending),
                "Initial status should be Pending"
            );
        }

        // Test sent method
        let log_data_sent = log_data.sent(claim_id.clone(), client_b_device_id.clone());

        // Verify claim still exists
        assert!(
            log_data_sent.claims.contains_key(&claim_id),
            "Log data should still contain our claim"
        );

        // Check if status was updated correctly
        let updated_claim = log_data_sent.claims.get(&claim_id).unwrap();
        let updated_status = updated_claim.status.get(&client_b_device_id).unwrap();
        assert!(
            matches!(updated_status, SsDistributionStatus::Sent),
            "Status should be updated to Sent"
        );

        // Status for other device should still be Pending
        let other_status = updated_claim.status.get(&vd_device_id).unwrap();
        assert!(
            matches!(other_status, SsDistributionStatus::Pending),
            "Status for other device should still be Pending"
        );

        // Test complete method
        let log_data_complete =
            log_data_sent.complete(claim_id.clone(), client_b_device_id.clone());

        // Verify claim still exists
        assert!(
            log_data_complete.claims.contains_key(&claim_id),
            "Log data should still contain our claim"
        );

        // Check if status was updated correctly
        let completed_claim = log_data_complete.claims.get(&claim_id).unwrap();
        let completed_status = completed_claim.status.get(&client_b_device_id).unwrap();
        assert!(
            matches!(completed_status, SsDistributionStatus::Delivered),
            "Status should be updated to Delivered"
        );

        // Status for other device should still be Pending
        let other_status_final = completed_claim.status.get(&vd_device_id).unwrap();
        assert!(
            matches!(other_status_final, SsDistributionStatus::Pending),
            "Status for other device should still be Pending"
        );

        // Test with non-existent claim ID
        let non_existent_id = ClaimId::from(Id48bit::generate());
        let log_data_non_existent =
            log_data_complete.sent(non_existent_id.clone(), client_b_device_id.clone());

        // Verify the operation is a no-op for non-existent claim
        assert_eq!(
            log_data_non_existent.claims.len(),
            1,
            "Log data should still contain only one claim"
        );
        assert!(
            !log_data_non_existent.claims.contains_key(&non_existent_id),
            "Log data should not contain the non-existent claim"
        );

        Ok(())
    }
}
