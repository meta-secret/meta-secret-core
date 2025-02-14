use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::vault::vault::VaultStatus;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
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
    pub async fn recovery_request(&self, pass_id: MetaPasswordId) -> anyhow::Result<()> {
        let user_creds = {
            let creds_repo = PersistentCredentials {
                p_obj: self.p_obj.clone(),
            };
            let maybe_user_creds = creds_repo.get_user_creds().await?;

            let Some(user_creds) = maybe_user_creds else {
                bail!("Invalid state. UserCredentials not exists")
            };

            user_creds
        };

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
                    .get_vault(member.user())
                    .await?
                    .to_data()
                    .to_vault_member(member)?;
                let claim = vault_member.create_recovery_claim(pass_id);

                let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
                p_ss.save_claim_in_ss_device_log(claim.clone()).await?;
            }
        }

        Ok(())
    }
}
