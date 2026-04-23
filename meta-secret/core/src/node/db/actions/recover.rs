use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::secret::{ClaimId, SecretDistributionData, SsDistributionId};
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
                let vault_member = vault_repo
                    .get_vault(member.user().vault_name())
                    .await?
                    .to_data()
                    .to_vault_member(member)?;
                let claim = vault_member.create_recovery_claim(pass_id);

                let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
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

        // Get recoveries and distributions from the claim
        let recoveries = p_ss.get_recoveries(claim.clone()).await?;

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

        Ok(plain_text)
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
