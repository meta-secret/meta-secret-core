use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::secret::{ClaimId, SecretDistributionData, SecretDistributionType, SsDistributionId, SsDistributionStatus};
use crate::node::common::model::user::user_creds::UserCreds;
use crate::node::common::model::vault::vault::VaultStatus;
use crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::recover_from_shares;
use crate::secret::shared_secret::UserShareDto;
use crate::PlainText;
use anyhow::bail;
use derive_more::From;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_attributes::instrument;

#[derive(From)]
pub struct RecoveryAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> RecoveryAction<Repo> {
    /// Send recover request to all vault members except current user
    #[instrument(skip_all)]
    pub async fn recovery_request(
        &self,
        user_creds: UserCreds,
        pass_id: MetaPasswordId,
    ) -> anyhow::Result<()> {
        let vault_repo = PersistentVault::from(self.p_obj.clone());

        let vault_status = vault_repo.find(user_creds.user()).await?;

        match vault_status {
            VaultStatus::NotExists(_) => {
                bail!("Vault not exists")
            }
            VaultStatus::Outsider(outsider) => {
                bail!("Outsider status: {:?}", outsider)
            }
            VaultStatus::Member(member) => {
                let sender_device_id = user_creds.device_id();
                let vault_name = user_creds.vault_name.clone();

                let p_ss = PersistentSharedSecret::from(self.p_obj.clone());

                // Retire stale Accepted claims before the dedup check.
                //
                // A claim with a Sent receiver is "Accepted" in compute_client_status.
                // If such a claim lingers (e.g. mark_claim_delivered never ran because
                // show_recovered crashed), two things go wrong on the next Recover click:
                //
                // 1. has_active_recovery_claim blocks new claim creation.
                // 2. waitForRecoveredClaim (Vue) calls isRecovered → find_recovery_claim
                //    returns the stale claim → returns true immediately without waiting
                //    for iOS to approve the new claim.
                // 3. show_recovered picks the stale claim (non-deterministic HashMap) →
                //    its recovery data is absent → only 1 share → "Invalid share".
                //
                // Marking stale claims Done here breaks the cycle: dedup passes, a fresh
                // claim is created, iOS approves it, and show_recovered gets valid data.
                {
                    info!("🔍 [recovery_request v2] sweep stale Accepted claims for pass_id={:?}", pass_id);
                    let ss_log_data = p_ss.get_ss_log_obj(vault_name.clone()).await?;
                    let stale: Vec<_> = ss_log_data
                        .claims
                        .values()
                        .filter(|c| {
                            c.distribution_type == SecretDistributionType::Recover
                                && &c.sender == sender_device_id
                                && c.dist_claim_id.pass_id == pass_id
                                && c.status
                                    .statuses
                                    .values()
                                    .any(|s| matches!(s, SsDistributionStatus::Sent))
                        })
                        .cloned()
                        .collect();

                    info!("🔍 [recovery_request v2] found {} stale claim(s) to retire", stale.len());

                    for claim in stale {
                        warn!("♻️ [recovery_request v2] retiring stale claim {:?}", claim.id);
                        let mut updated = claim.clone();
                        // Decline all Sent receivers to retire the stale claim.
                        // Using decline() rather than complete() avoids a fake Delivered status that
                        // would make compute_client_status return Done for the sender before
                        // show_recovered() has run — which triggers the wrong claim to be selected.
                        let sent_receivers: Vec<_> = updated
                            .status
                            .statuses
                            .iter()
                            .filter(|(_, s)| matches!(s, SsDistributionStatus::Sent))
                            .map(|(id, _)| id.clone())
                            .collect();
                        for receiver_id in sent_receivers {
                            updated.status = updated.status.decline(receiver_id);
                        }
                        p_ss.save_ss_log_event(updated).await?;
                    }
                }

                // Deduplication: ignore if an active claim already exists for (sender, pass_id).
                let ss_log = p_ss.get_ss_log_obj(vault_name).await?
                    .with_client_status(sender_device_id);
                if ss_log.has_active_recovery_claim(sender_device_id, &pass_id) {
                    return Ok(());
                }

                let vault_member = vault_repo
                    .get_vault(member.user().vault_name())
                    .await?
                    .to_data()
                    .to_vault_member(member)?;
                let claim = vault_member.create_recovery_claim(pass_id);
                p_ss.save_claim_in_ss_device_log(claim).await?;
            }
        }

        Ok(())
    }
}

/// Recovers secret from local shares on the client side
#[derive(From)]
pub struct RecoveryHandler<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> RecoveryHandler<Repo> {
    #[instrument(skip_all)]
    pub async fn recover(
        &self,
        user_creds: UserCreds,
        claim_id: ClaimId,
        pass_id: MetaPasswordId,
    ) -> anyhow::Result<PlainText> {
        info!("🔑 [recover v2] claim_id={:?} pass_id={:?}", claim_id, pass_id);

        // Create PersistentSharedSecret to access shared secret data
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());

        // 2. Get the SS log to find the claim
        let vault_name = user_creds.vault_name.clone();
        let ss_log_data = p_ss.get_ss_log_obj(vault_name).await?;

        // Find the claim using the ID in the recovery_id
        let claim = ss_log_data
            .claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("Claim not found for recovery ID"))?
            .clone();

        info!("🔑 [recover v2] claim statuses: {:?}", claim.status.statuses);

        // Get recoveries and distributions from the claim
        let recoveries = p_ss.get_recoveries(claim.clone()).await?;
        info!("🔑 [recover v2] recovery shares count: {}", recoveries.len());

        let desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id,
            receiver: user_creds.device_id().clone(),
        });
        let maybe_dist = self.p_obj.find_tail_event(desc).await?;

        // Extract all SecretDistributionData objects from recoveries and dists
        let recovery_data: Vec<SecretDistributionData> = recoveries
            .into_iter()
            .map(|r| r.to_distribution_data())
            .collect::<Result<Vec<_>, _>>()?;

        let distribution_data: Vec<SecretDistributionData> = maybe_dist
            .map(|dist| dist.to_distribution_data())
            .transpose()?
            .into_iter()
            .collect();

        info!(
            "🔑 [recover v2] own local share found: {}, distribution_data count: {}, recovery_data count: {}",
            distribution_data.len() > 0,
            distribution_data.len(),
            recovery_data.len()
        );

        if recovery_data.is_empty() && distribution_data.is_empty() {
            bail!("No recovery shares found for selected claim");
        }

        // Decrypt the secret shares using the transport key
        let transport_sk = &user_creds.device_creds.secret_box.transport.sk;

        // Prepare vectors to collect all shares
        let mut user_shares = Vec::new();

        // Process recovery shares
        for data in recovery_data {
            let decrypted = data.secret_message.cipher_text().decrypt(transport_sk)?;
            let share = UserShareDto::try_from(&decrypted.msg)?;
            user_shares.push(share);
        }

        // Process distribution shares
        for data in distribution_data {
            let decrypted = data.secret_message.cipher_text().decrypt(transport_sk)?;
            let share = UserShareDto::try_from(&decrypted.msg)?;
            user_shares.push(share);
        }

        // Recover the secret using the collected shares
        let plain_text = recover_from_shares(user_shares)?;

        // Mark the claim as Delivered so compute_client_status returns Done for the sender.
        // The server intentionally skips this transition for Recovery claims (see
        // server_data_sync.rs — "Completion event needs to be sent by the recovery claim creator").
        self.mark_claim_delivered(&user_creds, &claim_id).await?;

        Ok(plain_text)
    }

    async fn mark_claim_delivered(
        &self,
        user_creds: &UserCreds,
        claim_id: &ClaimId,
    ) -> anyhow::Result<()> {
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        let ss_log_data = p_ss.get_ss_log_obj(user_creds.vault_name.clone()).await?;

        let Some(current_claim) = ss_log_data.claims.get(claim_id) else {
            return Ok(());
        };
        let pass_id = current_claim.dist_claim_id.pass_id.clone();

        // Sweep ALL Recover claims for this pass_id and mark any stale Sent receiver as Done.
        // This handles claims from previous sessions that were never transitioned to Done
        // (e.g., approved before mark_claim_delivered was deployed), which would otherwise
        // block new claim creation and cause wrong claim selection on subsequent recoveries.
        let claims_to_update: Vec<_> = ss_log_data
            .claims
            .values()
            .filter(|c| {
                c.distribution_type == SecretDistributionType::Recover
                    && c.dist_claim_id.pass_id == pass_id
            })
            .cloned()
            .collect();

        for claim in claims_to_update {
            let receiver_to_mark = claim
                .status
                .statuses
                .iter()
                .find(|(_, s)| matches!(s, SsDistributionStatus::Sent))
                .map(|(id, _)| id.clone());

            if let Some(receiver_id) = receiver_to_mark {
                let mut updated_claim = claim.clone();
                updated_claim.status = updated_claim.status.complete(receiver_id);
                p_ss.save_ss_log_event(updated_claim).await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{RecoveryAction, RecoveryHandler};
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::crypto::aead::EncryptedMessage;
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::secret::{
        RecoveryClientStatus, SecretDistributionData, SecretDistributionType, SsDistributionId,
        SsDistributionStatus,
    };
    use crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
    use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
    use crate::node::db::events::shared_secret_event::SsWorkflowObject;
    use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
    use crate::node::db::repo::generic_db::SaveCommand;
    use crate::secret::data_block::common::SharedSecretConfig;
    use crate::secret::shared_secret::{PlainText, SharedSecretEncryption};
    use anyhow::Result;

    /// Stale sweep must decline (not complete) Sent receivers so the sender does NOT see Done
    /// prematurely. Using decline() instead of complete() ensures the sender stays in a state
    /// that blocks new claim creation (Sent is active, Declined is terminal). Without this,
    /// stale sweep would fake a Delivered status prematurely.
    #[test]
    fn stale_sweep_declines_sent_receiver_not_completes() {
        use crate::node::common::model::secret::{SsClaimId, SsClaim};
        use crate::crypto::utils::Id48bit;

        let registry = FixtureRegistry::empty();
        let sender = registry.state.device_creds.client.device.device_id.clone();
        let recv_a = registry.state.device_creds.client_b.device.device_id.clone();
        let pass_id = MetaPasswordId::build_from_str("stale_sweep_test");

        // Create a claim with Sent receiver (simulates previous recovery attempt)
        let claim_id = crate::node::common::model::secret::ClaimId::from(Id48bit::generate());
        let mut claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id.clone(),
                pass_id: pass_id.clone(),
            },
            vault_name: crate::node::common::model::vault::vault::VaultName::test(),
            sender: sender.clone(),
            distribution_type: SecretDistributionType::Recover,
            receivers: vec![recv_a.clone()],
            status: crate::node::common::model::secret::SsDistributionCompositeStatus::from(vec![recv_a.clone()]),
            client_status: None,
        };
        claim.status = claim.status.sent(recv_a.clone());

        // Simulate stale sweep with correct logic (decline, not complete)
        let mut retired = claim.clone();
        let sent_receivers: Vec<_> = retired
            .status
            .statuses
            .iter()
            .filter(|(_, s)| matches!(s, SsDistributionStatus::Sent))
            .map(|(id, _)| id.clone())
            .collect();
        for receiver_id in sent_receivers {
            retired.status = retired.status.decline(receiver_id);
        }

        // Verify the result: receiver must be Declined, not Delivered
        let recv_status = retired.status.statuses.get(&recv_a).unwrap();
        assert!(
            matches!(recv_status, SsDistributionStatus::Declined),
            "Stale sweep must Decline the Sent receiver. Got: {:?}",
            recv_status
        );

        // Sender must NOT see Done — Declined is terminal but not Done
        let log = crate::node::common::model::secret::SsLogData::new(retired)
            .with_client_status(&sender);
        let client_status = log
            .claims
            .get(&claim_id)
            .and_then(|c| c.client_status.as_ref());

        assert!(
            !matches!(client_status, Some(RecoveryClientStatus::Done)),
            "Sender must NOT see Done after declining stale claim. Got: {:?}",
            client_status
        );
    }

    #[tokio::test]
    async fn recover_works_without_local_distribution_when_recovery_shares_exist() -> Result<()> {
        let fixture = FixtureRegistry::empty();
        let user_creds = fixture.state.user_creds.client.clone();
        let p_obj = fixture.state.p_obj.client.clone();
        let p_ss = PersistentSharedSecret::from(p_obj.clone());
        let vault_member = fixture.state.vault_data.client_vault_member.clone();

        let pass_id = MetaPasswordId::build_from_str("recover_without_local_distribution");
        let claim = vault_member.create_recovery_claim(pass_id.clone());
        p_ss.save_ss_log_event(claim.clone()).await?;

        let local_distribution_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: pass_id.clone(),
            receiver: user_creds.device_id().clone(),
        });
        let local_distribution = p_obj.find_tail_event(local_distribution_desc).await?;
        assert!(
            local_distribution.is_none(),
            "Test precondition: local distribution share must be absent"
        );

        let cfg = SharedSecretConfig {
            number_of_shares: 2,
            threshold: 2,
        };
        let shared_secret = SharedSecretEncryption::new(cfg, PlainText::from("2bee|~"))?;
        let sender_pk = user_creds.device_creds.device.keys.transport_pk();
        let sender_km = user_creds.device_creds.key_manager()?;

        for (share_index, recovery_id) in claim.recovery_db_ids().into_iter().enumerate() {
            let share_json = shared_secret.get_share(share_index).as_json()?;
            let encrypted = sender_km
                .transport
                .encrypt_string(PlainText::from(share_json), &sender_pk)?;

            let wf_event = SsWorkflowObject::Recovery(KvLogEvent {
                key: KvKey::from(SsWorkflowDescriptor::Recovery(recovery_id.clone())),
                value: SecretDistributionData {
                    vault_name: user_creds.vault_name.clone(),
                    claim_id: recovery_id.claim_id,
                    secret_message: EncryptedMessage::CipherShare { share: encrypted },
                },
            });

            p_obj.repo.save(wf_event).await?;
        }

        let recovery = RecoveryHandler { p_obj };
        let plain = recovery
            .recover(user_creds, claim.id, pass_id)
            .await?;

        assert_eq!(plain.text, "2bee|~");
        Ok(())
    }

    #[tokio::test]
    async fn recover_fails_when_no_recovery_and_no_local_distribution() -> Result<()> {
        let fixture = FixtureRegistry::empty();
        let user_creds = fixture.state.user_creds.client.clone();
        let p_obj = fixture.state.p_obj.client.clone();
        let p_ss = PersistentSharedSecret::from(p_obj.clone());
        let vault_member = fixture.state.vault_data.client_vault_member.clone();

        let pass_id = MetaPasswordId::build_from_str("recover_without_any_shares");
        let claim = vault_member.create_recovery_claim(pass_id.clone());
        p_ss.save_ss_log_event(claim.clone()).await?;

        let local_distribution_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: pass_id.clone(),
            receiver: user_creds.device_id().clone(),
        });
        let local_distribution = p_obj.find_tail_event(local_distribution_desc).await?;
        assert!(
            local_distribution.is_none(),
            "Test precondition: local distribution share must be absent"
        );

        let recovery = RecoveryHandler { p_obj };
        let err = recovery
            .recover(user_creds, claim.id, pass_id)
            .await
            .expect_err("Recover must fail when no shares are available");

        assert!(
            err.to_string().contains("No recovery shares found for selected claim"),
            "Unexpected error: {err}"
        );

        Ok(())
    }
}
