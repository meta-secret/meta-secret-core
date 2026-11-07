#[path = "recovery_capture.rs"]
mod capture;
use crate::PlainText;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::secret::{ClaimId, SecretDistributionData, SsDistributionId};
use crate::node::common::model::user::user_creds::UserCreds;
use crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::recover_from_shares;
use crate::secret::shared_secret::UserShareDto;
use anyhow::bail;
use derive_more::From;
use std::sync::Arc;
use tracing::info;
use tracing_attributes::instrument;

#[derive(From)]
pub struct RecoveryAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> RecoveryAction<Repo> {
    pub async fn recovery_request(
        &self,
        creds: UserCreds,
        pass: MetaPasswordId,
    ) -> anyhow::Result<()> {
        self.recovery_request_checked(creds, pass)
            .await?
            .require_queued()
    }
}

/// Recovers secret from local shares on the client side
#[derive(From)]
pub struct RecoveryHandler<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> RecoveryHandler<Repo> {
    /// Recover a secret using the exact claim selected by Core.
    ///
    /// The Pass ID is intentionally derived from the persisted claim instead of
    /// being supplied by a client. This prevents a caller from pairing one
    /// Claim ID with a different Pass ID and guarantees that the recovery
    /// distribution lookup and completion refer to the same claim.
    pub async fn recover_by_claim_id(
        &self,
        user_creds: UserCreds,
        claim_id: ClaimId,
    ) -> anyhow::Result<PlainText> {
        Ok(self
            .recover_by_claim_id_checked(user_creds, claim_id)
            .await?
            .plaintext)
    }

    pub async fn recover_by_claim_id_checked(
        &self,
        user_creds: UserCreds,
        claim_id: ClaimId,
    ) -> anyhow::Result<crate::node::common::model::liveboard_reveal::RecoveredReveal> {
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        let ss_log_data = p_ss.get_ss_log_obj(user_creds.vault_name.clone()).await?;
        let claim = ss_log_data
            .claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("Claim not found for recovery ID"))?;

        if claim.distribution_type
            != crate::node::common::model::secret::SecretDistributionType::Recover
        {
            bail!("Claim is not a recovery claim");
        }
        if claim.sender != *user_creds.device_id() {
            bail!("Recovery claim belongs to another device");
        }
        PersistentVault::from(self.p_obj.clone())
            .get_vault(user_creds.vault_name.clone())
            .await?
            .to_data()
            .ensure_secret_available(&claim.dist_claim_id.pass_id)?;

        self.recover_captured(user_creds, claim_id, claim.dist_claim_id.pass_id.clone())
            .await
    }

    #[instrument(skip_all)]
    pub async fn recover(
        &self,
        user_creds: UserCreds,
        claim_id: ClaimId,
        pass_id: MetaPasswordId,
    ) -> anyhow::Result<PlainText> {
        Ok(self
            .recover_captured(user_creds, claim_id, pass_id)
            .await?
            .plaintext)
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
        let mut updated_claim = current_claim.clone();
        updated_claim.status = updated_claim
            .status
            .complete(user_creds.device_id().clone());
        p_ss.save_ss_log_event(updated_claim).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::RecoveryHandler;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::crypto::aead::EncryptedMessage;
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::secret::{SecretDistributionData, SsDistributionId};
    use crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
    use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
    use crate::node::db::events::shared_secret_event::SsWorkflowObject;
    use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
    use crate::node::db::repo::generic_db::SaveCommand;
    use crate::secret::data_block::common::SharedSecretConfig;
    use crate::secret::shared_secret::{PlainText, SharedSecretEncryption};
    use anyhow::Result;

    #[tokio::test]
    async fn recover_works_without_local_distribution_when_recovery_shares_exist() -> Result<()> {
        let fixture = FixtureRegistry::empty();
        let user_creds = fixture.state.user_creds.client.clone();
        let p_obj = fixture.state.p_obj.client.clone();
        let p_ss = PersistentSharedSecret::from(p_obj.clone());
        let vault_member = fixture.state.vault_data.client_vault_member.clone();

        let pass_id = MetaPasswordId::build_from_str("recover_without_local_distribution");
        let mut vault = crate::node::db::events::vault::vault_event::VaultObject::sign_up(
            user_creds.vault_name.clone(),
            vault_member.member.clone(),
        );
        vault.0.value = vault_member.vault.clone().add_secret(pass_id.clone());
        p_obj.repo.save(vault).await?;
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
                .encrypt_string(PlainText::from(share_json), sender_pk)?;

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
        let plain = recovery.recover_by_claim_id(user_creds, claim.id).await?;

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
        let mut vault = crate::node::db::events::vault::vault_event::VaultObject::sign_up(
            user_creds.vault_name.clone(),
            vault_member.member.clone(),
        );
        vault.0.value = vault_member.vault.clone().add_secret(pass_id.clone());
        p_obj.repo.save(vault).await?;
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
            .recover_by_claim_id(user_creds, claim.id)
            .await
            .expect_err("Recover must fail when no shares are available");

        assert!(
            err.to_string()
                .contains("No recovery shares found for selected claim"),
            "Unexpected error: {err}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn recover_by_claim_id_rejects_a_claim_owned_by_another_sender() -> Result<()> {
        let fixture = FixtureRegistry::empty();
        let user_creds = fixture.state.user_creds.client.clone();
        let p_obj = fixture.state.p_obj.client.clone();
        let p_ss = PersistentSharedSecret::from(p_obj.clone());
        let vault_member = fixture.state.vault_data.client_vault_member.clone();

        let pass_id = MetaPasswordId::build_from_str("claim_sender_binding");
        let mut claim = vault_member.create_recovery_claim(pass_id);
        claim.sender = fixture.state.device_creds.client_b.device.device_id.clone();
        p_ss.save_ss_log_event(claim.clone()).await?;

        let recovery = RecoveryHandler { p_obj };
        let err = recovery
            .recover_by_claim_id(user_creds, claim.id)
            .await
            .expect_err("a sender must not recover another device's claim");

        assert!(err.to_string().contains("belongs to another device"));
        Ok(())
    }
}
