use crate::crypto::utils::Id48bit;
use crate::node::common::model::crypto::aead::EncryptedMessage;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::common::model::IdString;
use derive_more::From;
use std::collections::HashMap;
use wasm_bindgen::prelude::wasm_bindgen;
use tracing::debug;

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
    #[serde(skip_deserializing, default, skip_serializing_if = "Option::is_none")]
    pub client_status: Option<RecoveryClientStatus>,
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

    pub fn compute_client_status(&self, current_device: &DeviceId) -> Option<RecoveryClientStatus> {
        if self.distribution_type != SecretDistributionType::Recover {
            return None;
        }

        let is_sender = &self.sender == current_device;
        let is_receiver = self.receivers.contains(current_device);

        let total = self.receivers.len();
        let num_sent = self
            .status
            .statuses
            .values()
            .filter(|s| matches!(s, SsDistributionStatus::Sent))
            .count();
        let num_delivered = self
            .status
            .statuses
            .values()
            .filter(|s| matches!(s, SsDistributionStatus::Delivered))
            .count();
        let num_declined = self
            .status
            .statuses
            .values()
            .filter(|s| matches!(s, SsDistributionStatus::Declined))
            .count();

        debug!(
            claim_id = ?self.id,
            is_sender,
            is_receiver,
            num_sent,
            num_delivered,
            num_declined,
            total,
            "compute_client_status: evaluating claim"
        );

        let result = if is_sender {
            if num_delivered >= 1 {
                // Sender retrieved the secret — claim is finished
                Some(RecoveryClientStatus::Done)
            } else if num_sent >= 1 {
                Some(RecoveryClientStatus::Accepted)
            } else if num_declined >= total {
                Some(RecoveryClientStatus::Declined)
            } else {
                Some(RecoveryClientStatus::Pending)
            }
        } else if is_receiver {
            if num_sent >= 1 {
                Some(RecoveryClientStatus::Done)
            } else {
                match self.status.statuses.get(current_device) {
                    Some(SsDistributionStatus::Pending) | None => Some(RecoveryClientStatus::NeedApprove),
                    _ => Some(RecoveryClientStatus::Done),
                }
            }
        } else {
            None
        };

        debug!(claim_id = ?self.id, ?result, "compute_client_status: result");
        result
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
    /// The receiver device has declined the recovery request
    Declined,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryClientStatus {
    Pending,      // sender: waiting for receivers (show loader)
    NeedApprove,  // receiver: show 'Allow recovery?' alert
    Accepted,     // sender: call showRecovered()
    Declined,     // sender: show 'recovery declined' notification
    Done,         // sender: secret retrieved (Delivered); receiver: dismiss alerts/loaders
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

    pub fn decline(mut self, device_id: DeviceId) -> Self {
        println!("🦀 Insert decline for device_id: {:?}", device_id);
        self.statuses
            .insert(device_id, SsDistributionStatus::Declined);
        self
    }

    pub fn get(&self, device_id: &DeviceId) -> Option<&SsDistributionStatus> {
        self.statuses.get(device_id)
    }

    pub fn status(&self) -> SsDistributionStatus {
        let is_pending = self
            .statuses
            .values()
            .any(|dist_status| matches!(dist_status, SsDistributionStatus::Pending));

        let is_sent = self
            .statuses
            .values()
            .any(|dist_status| matches!(dist_status, SsDistributionStatus::Sent));

        let is_declined = self
            .statuses
            .values()
            .any(|dist_status| matches!(dist_status, SsDistributionStatus::Declined));

        // TODO: k-of-N — replace with count(Sent) + count(Delivered) >= threshold
        if is_sent {
            SsDistributionStatus::Sent
        } else if is_pending {
            SsDistributionStatus::Pending
        } else if is_declined {
            SsDistributionStatus::Declined
        } else {
            SsDistributionStatus::Delivered
        }
    }

    pub fn decline_remaining_pending(mut self) -> Self {
        // TODO: k-of-N — call only when count(Sent) >= threshold instead of any(Sent)
        for status in self.statuses.values_mut() {
            if matches!(status, SsDistributionStatus::Pending) {
                *status = SsDistributionStatus::Declined;
            }
        }
        self
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDeclineData {
    pub vault_name: VaultName,
    pub claim_id: ClaimId,
    pub receiver_id: DeviceId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsLogData {
    pub claims: HashMap<ClaimId, SsClaim>,
}

impl SsLogData {
    pub fn find_recovery_claim_id(&self, pass_id: &MetaPasswordId) -> Option<ClaimId> {
        // Use client_status (populated by with_client_status()) to select only active claims.
        // This avoids the HashMap non-determinism bug: when multiple claims exist for the same
        // pass_id (e.g. a stale retired claim and a fresh active one), only Pending/Accepted
        // claims are candidates. Done and Declined claims are terminal and must be skipped.
        for (_, claim) in self.claims.iter() {
            let SecretDistributionType::Recover = claim.distribution_type else {
                continue;
            };
            if !pass_id.eq(&claim.dist_claim_id.pass_id) {
                continue;
            }
            match claim.client_status {
                Some(RecoveryClientStatus::Pending) | Some(RecoveryClientStatus::Accepted) => {
                    return Some(claim.id.clone());
                }
                _ => continue,
            }
        }
        None
    }

    pub fn find_recovery_claim(&self, pass_id: &MetaPasswordId) -> Option<SsClaim> {
        let mut result_claim = None;
        for (_, claim) in self.claims.iter() {
            let SecretDistributionType::Recover = claim.distribution_type else {
                continue;
            };

            let claim_pass_id = &claim.dist_claim_id.pass_id;

            if !pass_id.eq(claim_pass_id) {
                continue;
            }

            match claim.status.status() {
                SsDistributionStatus::Pending => {
                    println!("🦀 Founded claim status: Pending");
                    result_claim = Some(claim.clone());
                    break;
                }
                SsDistributionStatus::Sent => {
                    println!("🦀 Founded claim status: Sent");
                    result_claim= Some(claim.clone());
                    break;
                }
                SsDistributionStatus::Delivered => {
                    println!("🦀 Founded claim status: Delivered");
                    result_claim = Some(claim.clone());
                    break;
                }
                SsDistributionStatus::Declined => {
                    println!("🦀 Founded claim status: Declined");
                    result_claim = Some(claim.clone());
                    break;
                }
            }
        }

        result_claim
    }
}

impl SsLogData {
    pub fn sent(mut self, claim_id: ClaimId, device_id: DeviceId) -> Self {
        let maybe_claim = self.claims.remove(&claim_id);

        if let Some(mut claim) = maybe_claim {
            claim.status = claim.status.sent(device_id);
            self.claims.insert(claim_id, claim);
        }

        self
    }

    pub fn decline_remaining_pending(mut self, claim_id: ClaimId) -> Self {
        let maybe_claim = self.claims.remove(&claim_id);

        if let Some(mut claim) = maybe_claim {
            // TODO: k-of-N — call only when count(Sent) >= threshold
            claim.status = claim.status.decline_remaining_pending();
            self.claims.insert(claim_id, claim);
        }

        self
    }

    pub fn complete(mut self, claim_id: ClaimId, device_id: DeviceId) -> Self {
        let maybe_claim = self.claims.remove(&claim_id);

        if let Some(mut claim) = maybe_claim {
            claim.status = claim.status.complete(device_id);
            // Always re-insert: keep claim even when fully Delivered so future redistributions
            // and recovery operations can find the distribution history.
            self.claims.insert(claim_id, claim);
        }

        self
    }

    /// Complete recovery with specific receiver status (Sent for accept, Declined for decline)
    pub fn complete_with_receiver_status(
        mut self, 
        claim_id: ClaimId, 
        sender_id: DeviceId, 
        receiver_id: DeviceId,
        receiver_status: SsDistributionStatus,
    ) -> Self {
        let maybe_claim = self.claims.remove(&claim_id);

        if let Some(mut claim) = maybe_claim {
            // Set receiver status (Sent or Declined)
            claim.status.statuses.insert(receiver_id, receiver_status);
            // Set sender status to Delivered
            claim.status = claim.status.complete(sender_id);

            // Always re-insert: keep claim even when fully Delivered so future redistributions
            // and recovery operations can find the distribution history.
            self.claims.insert(claim_id, claim);
        }

        self
    }

    pub fn decline(mut self, claim_id: ClaimId, device_id: DeviceId) -> Self {
        let maybe_claim = self.claims.remove(&claim_id);

        if let Some(mut claim) = maybe_claim {
            claim.status = claim.status.decline(device_id);
            // Keep the claim with Declined status so the sender can see the rejection
            self.claims.insert(claim_id, claim);
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

    pub fn with_client_status(mut self, current_device: &DeviceId) -> Self {
        let total = self.claims.len();
        for claim in self.claims.values_mut() {
            claim.client_status = claim.compute_client_status(current_device);
        }
        let computed = self.claims.values().filter(|c| c.client_status.is_some()).count();
        debug!(total, computed, "with_client_status: populated client_status for Recover claims");
        self
    }

    /// Returns true if there is already an active recovery claim for the given sender and secret.
    /// Active = not Declined and not Done (i.e. Pending, NeedApprove, or Accepted).
    /// Must be called after `with_client_status()` so `client_status` is populated.
    pub fn has_active_recovery_claim(&self, sender: &DeviceId, pass_id: &MetaPasswordId) -> bool {
        self.claims.values().any(|claim| {
            matches!(claim.distribution_type, SecretDistributionType::Recover)
                && &claim.sender == sender
                && claim.dist_claim_id.pass_id == *pass_id
                && !matches!(
                    claim.client_status,
                    Some(RecoveryClientStatus::Declined) | Some(RecoveryClientStatus::Done)
                )
        })
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
        ClaimId, RecoveryClientStatus, SecretDistributionType, SsClaim, SsClaimId,
        SsDistributionCompositeStatus, SsDistributionStatus, SsLogData,
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
            client_status: None,
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
            client_status: None,
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
        // Aggregate uses any(Sent) -> Sent (see SsDistributionCompositeStatus::status).
        // TODO(k-of-N): when threshold-based aggregation is implemented, revisit this expectation.
        assert!(
            matches!(mixed_statuses.status(), SsDistributionStatus::Sent),
            "Overall status should be Sent when any receiver is Sent (Pending+Sent mix)"
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
            client_status: None,
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

    fn make_recover_claim(
        sender: DeviceId,
        receivers: Vec<DeviceId>,
    ) -> (SsClaim, ClaimId) {
        let claim_id = ClaimId::from(Id48bit::generate());
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
            sender,
            distribution_type: SecretDistributionType::Recover,
            receivers: receivers.clone(),
            status: SsDistributionCompositeStatus::from(receivers),
            client_status: None,
        };
        (claim, claim_id)
    }

    #[test]
    fn test_compute_client_status_split_returns_none() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let receiver = registry.state.device_creds.client_b.device.device_id;

        let claim_id = ClaimId::from(Id48bit::generate());
        let split_claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id,
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("p".to_string()),
                    name: "p".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender: sender.clone(),
            distribution_type: SecretDistributionType::Split,
            receivers: vec![receiver.clone()],
            status: SsDistributionCompositeStatus::from(vec![receiver.clone()]),
            client_status: None,
        };

        assert_eq!(split_claim.compute_client_status(&sender), None);
        assert_eq!(split_claim.compute_client_status(&receiver), None);
    }

    #[test]
    fn test_compute_client_status_sender_pending() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;

        let (claim, _) = make_recover_claim(sender.clone(), vec![recv_a, recv_b]);
        // All receivers still Pending → sender sees Pending
        assert_eq!(
            claim.compute_client_status(&sender),
            Some(RecoveryClientStatus::Pending)
        );
    }

    #[test]
    fn test_compute_client_status_sender_accepted_when_one_sent() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;
        let claim_id = ClaimId::from(Id48bit::generate());

        let mut claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id,
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("p".to_string()),
                    name: "p".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender: sender.clone(),
            distribution_type: SecretDistributionType::Recover,
            receivers: vec![recv_a.clone(), recv_b.clone()],
            status: SsDistributionCompositeStatus::from(vec![recv_a.clone(), recv_b.clone()]),
            client_status: None,
        };
        // One receiver approves (Sent)
        claim.status = claim.status.sent(recv_a.clone());

        assert_eq!(
            claim.compute_client_status(&sender),
            Some(RecoveryClientStatus::Accepted)
        );
    }

    #[test]
    fn test_compute_client_status_sender_done_when_one_delivered() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;
        let claim_id = ClaimId::from(Id48bit::generate());

        let mut claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id,
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("p".to_string()),
                    name: "p".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender: sender.clone(),
            distribution_type: SecretDistributionType::Recover,
            receivers: vec![recv_a.clone(), recv_b.clone()],
            status: SsDistributionCompositeStatus::from(vec![recv_a.clone(), recv_b.clone()]),
            client_status: None,
        };
        // Delivered means sender retrieved the secret — claim is finished
        claim.status = claim.status.complete(recv_a.clone());

        assert_eq!(
            claim.compute_client_status(&sender),
            Some(RecoveryClientStatus::Done)
        );
    }

    #[test]
    fn test_compute_client_status_sender_declined_when_all_declined() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;
        let claim_id = ClaimId::from(Id48bit::generate());

        let mut claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id,
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("p".to_string()),
                    name: "p".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender: sender.clone(),
            distribution_type: SecretDistributionType::Recover,
            receivers: vec![recv_a.clone(), recv_b.clone()],
            status: SsDistributionCompositeStatus::from(vec![recv_a.clone(), recv_b.clone()]),
            client_status: None,
        };
        claim.status = claim.status.decline(recv_a.clone()).decline(recv_b.clone());

        assert_eq!(
            claim.compute_client_status(&sender),
            Some(RecoveryClientStatus::Declined)
        );
    }

    #[test]
    fn test_compute_client_status_sender_pending_when_only_some_declined() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;
        let claim_id = ClaimId::from(Id48bit::generate());

        let mut claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id,
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("p".to_string()),
                    name: "p".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender: sender.clone(),
            distribution_type: SecretDistributionType::Recover,
            receivers: vec![recv_a.clone(), recv_b.clone()],
            status: SsDistributionCompositeStatus::from(vec![recv_a.clone(), recv_b.clone()]),
            client_status: None,
        };
        // Only one of two declined → still Pending for sender
        claim.status = claim.status.decline(recv_a.clone());

        assert_eq!(
            claim.compute_client_status(&sender),
            Some(RecoveryClientStatus::Pending)
        );
    }

    #[test]
    fn test_compute_client_status_receiver_need_approve() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;

        let (claim, _) =
            make_recover_claim(sender, vec![recv_a.clone(), recv_b.clone()]);
        // All Pending → recv_a should see NeedApprove
        assert_eq!(
            claim.compute_client_status(&recv_a),
            Some(RecoveryClientStatus::NeedApprove)
        );
    }

    #[test]
    fn test_compute_client_status_receiver_done_when_claim_resolved() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;
        let claim_id = ClaimId::from(Id48bit::generate());

        let mut claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id,
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("p".to_string()),
                    name: "p".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender,
            distribution_type: SecretDistributionType::Recover,
            receivers: vec![recv_a.clone(), recv_b.clone()],
            status: SsDistributionCompositeStatus::from(vec![recv_a.clone(), recv_b.clone()]),
            client_status: None,
        };
        // recv_a approved → recv_b sees Done even though recv_b is still Pending
        claim.status = claim.status.sent(recv_a.clone());

        assert_eq!(
            claim.compute_client_status(&recv_b),
            Some(RecoveryClientStatus::Done)
        );
        // recv_a itself also sees Done (own status Sent, claim_resolved)
        assert_eq!(
            claim.compute_client_status(&recv_a),
            Some(RecoveryClientStatus::Done)
        );
    }

    #[test]
    fn test_compute_client_status_receiver_done_after_own_decline() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;
        let claim_id = ClaimId::from(Id48bit::generate());

        let mut claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id,
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("p".to_string()),
                    name: "p".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender,
            distribution_type: SecretDistributionType::Recover,
            receivers: vec![recv_a.clone(), recv_b.clone()],
            status: SsDistributionCompositeStatus::from(vec![recv_a.clone(), recv_b.clone()]),
            client_status: None,
        };
        // recv_a declined → recv_a sees Done (already acted)
        claim.status = claim.status.decline(recv_a.clone());

        assert_eq!(
            claim.compute_client_status(&recv_a),
            Some(RecoveryClientStatus::Done)
        );
    }

    #[test]
    fn test_compute_client_status_unknown_device_returns_none() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let (claim, _) = make_recover_claim(sender, vec![recv_a]);

        let outsider = DeviceId(U64IdUrlEnc::from("unknown_device".to_string()));
        assert_eq!(claim.compute_client_status(&outsider), None);
    }

    #[test]
    fn test_with_client_status_populates_all_recover_claims() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;

        let (claim1, _) = make_recover_claim(sender.clone(), vec![recv_a.clone(), recv_b.clone()]);
        // Split claim — should remain without client_status
        let split_id = ClaimId::from(Id48bit::generate());
        let split_claim = SsClaim {
            id: split_id.clone(),
            dist_claim_id: SsClaimId {
                id: split_id,
                pass_id: MetaPasswordId {
                    id: U64IdUrlEnc::from("s".to_string()),
                    name: "s".to_string(),
                },
            },
            vault_name: VaultName::test(),
            sender: sender.clone(),
            distribution_type: SecretDistributionType::Split,
            receivers: vec![recv_a.clone()],
            status: SsDistributionCompositeStatus::from(vec![recv_a.clone()]),
            client_status: None,
        };

        let log = SsLogData::new(claim1).insert(split_claim);
        let log = log.with_client_status(&sender);

        for claim in log.claims.values() {
            match claim.distribution_type {
                SecretDistributionType::Recover => {
                    assert!(
                        claim.client_status.is_some(),
                        "Recover claim must have client_status populated"
                    );
                    assert_eq!(claim.client_status, Some(RecoveryClientStatus::Pending));
                }
                SecretDistributionType::Split => {
                    assert!(
                        claim.client_status.is_none(),
                        "Split claim must not have client_status"
                    );
                }
            }
        }
    }

    fn make_pass_id(name: &str) -> MetaPasswordId {
        MetaPasswordId {
            id: U64IdUrlEnc::from(name.to_string()),
            name: name.to_string(),
        }
    }

    #[test]
    fn test_has_active_recovery_claim_pending_is_active() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv = registry.state.device_creds.client_b.device.device_id;
        let pass_id = make_pass_id("secret1");

        let (mut claim, _) = make_recover_claim(sender.clone(), vec![recv]);
        claim.dist_claim_id.pass_id = pass_id.clone();

        let log = SsLogData::new(claim).with_client_status(&sender);

        assert!(log.has_active_recovery_claim(&sender, &pass_id));
    }

    #[test]
    fn test_has_active_recovery_claim_accepted_is_active() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv = registry.state.device_creds.client_b.device.device_id;
        let pass_id = make_pass_id("secret1");

        let (mut claim, claim_id) = make_recover_claim(sender.clone(), vec![recv.clone()]);
        claim.dist_claim_id.pass_id = pass_id.clone();

        // Receiver approved (Sent) → sender sees Accepted → still active until secret is read (Done)
        let log = SsLogData::new(claim).sent(claim_id, recv);
        let log = log.with_client_status(&sender);

        assert!(log.has_active_recovery_claim(&sender, &pass_id));
    }

    #[test]
    fn test_has_active_recovery_claim_done_is_not_active() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv = registry.state.device_creds.client_b.device.device_id;
        let pass_id = make_pass_id("secret1");

        let (mut claim, claim_id) = make_recover_claim(sender.clone(), vec![recv.clone()]);
        claim.dist_claim_id.pass_id = pass_id.clone();

        // Sender retrieved secret (Delivered) → Done
        let log = SsLogData::new(claim).complete(claim_id, recv);
        let log = log.with_client_status(&sender);

        assert!(!log.has_active_recovery_claim(&sender, &pass_id));
    }

    #[test]
    fn test_has_active_recovery_claim_declined_is_not_active() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;
        let pass_id = make_pass_id("secret1");

        let (mut claim, claim_id) =
            make_recover_claim(sender.clone(), vec![recv_a.clone(), recv_b.clone()]);
        claim.dist_claim_id.pass_id = pass_id.clone();

        // All declined → Declined
        let log = SsLogData::new(claim)
            .decline(claim_id.clone(), recv_a)
            .decline(claim_id, recv_b);
        let log = log.with_client_status(&sender);

        assert!(!log.has_active_recovery_claim(&sender, &pass_id));
    }

    #[test]
    fn test_has_active_recovery_claim_different_pass_id_is_ignored() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv = registry.state.device_creds.client_b.device.device_id;
        let pass_id = make_pass_id("secret1");
        let other_pass_id = make_pass_id("secret2");

        let (mut claim, _) = make_recover_claim(sender.clone(), vec![recv]);
        claim.dist_claim_id.pass_id = pass_id.clone();

        let log = SsLogData::new(claim).with_client_status(&sender);

        // Active claim exists for secret1, but not for secret2
        assert!(log.has_active_recovery_claim(&sender, &pass_id));
        assert!(!log.has_active_recovery_claim(&sender, &other_pass_id));
    }

    #[test]
    fn test_find_recovery_claim_id_returns_accepted_claim() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv = registry.state.device_creds.client_b.device.device_id;
        let pass_id = make_pass_id("secret1");

        let (mut claim, claim_id) = make_recover_claim(sender.clone(), vec![recv.clone()]);
        claim.dist_claim_id.pass_id = pass_id.clone();

        let log = SsLogData::new(claim).sent(claim_id, recv).with_client_status(&sender);

        let found = log.find_recovery_claim_id(&pass_id);
        assert!(found.is_some(), "Should find the Accepted (Sent) claim");
    }

    #[test]
    fn test_find_recovery_claim_id_skips_done_claim_prefers_accepted() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;
        let pass_id = make_pass_id("secret1");

        // Claim #1: Done (Delivered) — stale from previous session
        let (mut claim1, claim_id1) = make_recover_claim(sender.clone(), vec![recv_a.clone()]);
        claim1.dist_claim_id.pass_id = pass_id.clone();
        let log = SsLogData::new(claim1).complete(claim_id1, recv_a.clone());

        // Claim #2: Accepted (Sent) — active
        let (mut claim2, claim_id2) = make_recover_claim(sender.clone(), vec![recv_b.clone()]);
        claim2.dist_claim_id.pass_id = pass_id.clone();
        let log = log.insert(claim2).sent(claim_id2.clone(), recv_b.clone()).with_client_status(&sender);

        let found = log.find_recovery_claim_id(&pass_id);
        assert_eq!(found, Some(claim_id2), "Must return active claim, not the Done one");
    }

    #[test]
    fn test_find_recovery_claim_id_returns_none_when_all_done() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv = registry.state.device_creds.client_b.device.device_id;
        let pass_id = make_pass_id("secret1");

        let (mut claim, claim_id) = make_recover_claim(sender.clone(), vec![recv.clone()]);
        claim.dist_claim_id.pass_id = pass_id.clone();
        let log = SsLogData::new(claim).complete(claim_id, recv).with_client_status(&sender);

        let found = log.find_recovery_claim_id(&pass_id);
        assert!(found.is_none(), "All Done → no active claim to recover from");
    }

    #[test]
    fn test_find_recovery_claim_id_skips_declined_claim_prefers_pending() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let recv_a = registry.state.device_creds.client_b.device.device_id;
        let recv_b = registry.state.device_creds.vd.device.device_id;
        let pass_id = make_pass_id("secret1");

        // Claim #1: Declined — stale claim retired by stale sweep (now using decline())
        let (mut claim1, claim_id1) = make_recover_claim(sender.clone(), vec![recv_a.clone()]);
        claim1.dist_claim_id.pass_id = pass_id.clone();
        let log = SsLogData::new(claim1).decline(claim_id1, recv_a.clone());

        // Claim #2: Pending — fresh claim just created
        let (mut claim2, claim_id2) = make_recover_claim(sender.clone(), vec![recv_b.clone()]);
        claim2.dist_claim_id.pass_id = pass_id.clone();
        let log = log.insert(claim2).with_client_status(&sender);

        let found = log.find_recovery_claim_id(&pass_id);
        assert_eq!(found, Some(claim_id2), "Must return Pending claim, not the Declined stale one");
    }

    #[test]
    fn test_has_active_recovery_claim_different_sender_is_ignored() {
        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id;
        let other_sender = registry.state.device_creds.vd.device.device_id;
        let recv = registry.state.device_creds.client_b.device.device_id;
        let pass_id = make_pass_id("secret1");

        let (mut claim, _) = make_recover_claim(sender.clone(), vec![recv]);
        claim.dist_claim_id.pass_id = pass_id.clone();

        let log = SsLogData::new(claim).with_client_status(&sender);

        assert!(log.has_active_recovery_claim(&sender, &pass_id));
        assert!(!log.has_active_recovery_claim(&other_sender, &pass_id));
    }
}
