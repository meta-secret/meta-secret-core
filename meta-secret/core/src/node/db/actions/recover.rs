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
        let dist = self
            .p_obj
            .find_tail_event(desc)
            .await?
            .unwrap()
            .to_distribution_data();

        // Extract all SecretDistributionData objects from recoveries and dists
        let recovery_data: Vec<SecretDistributionData> = recoveries
            .into_iter()
            .map(|r| r.to_distribution_data())
            .collect();

        let distribution_data = vec![dist];

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
