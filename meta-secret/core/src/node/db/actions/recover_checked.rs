//! Actual RecoveryAction claim ownership, with sanitized persistence outcome.
use super::recover::RecoveryAction;
use crate::node::common::model::liveboard::{LiveboardOperationKind, LiveboardOperationReceipt};
use crate::node::common::model::{
    meta_pass::MetaPasswordId, user::user_creds::UserCreds, vault::vault::VaultStatus,
};
use crate::node::db::{
    descriptors::shared_secret_descriptor::SsDeviceLogDescriptor,
    events::shared_secret_event::SsDeviceLogObject,
    objects::{
        persistent_shared_secret::PersistentSharedSecret, persistent_vault::PersistentVault,
    },
    repo::generic_db::KvLogEventRepo,
};
use anyhow::bail;
use tracing::info;
use tracing_attributes::instrument;

#[path = "recover_ownership.rs"]
mod ownership;

impl<R: KvLogEventRepo> RecoveryAction<R> {
    /// Send recover request to all vault members except current user
    #[instrument(skip_all)]
    pub async fn recovery_request_checked(
        &self,
        user_creds: UserCreds,
        pass_id: MetaPasswordId,
    ) -> anyhow::Result<LiveboardOperationReceipt> {
        let mut receipt = LiveboardOperationReceipt::queued(LiveboardOperationKind::Recover);
        receipt.secret_id = Some(pass_id.id_str());
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
                vault_repo
                    .get_vault(member.user().vault_name())
                    .await?
                    .to_data()
                    .ensure_secret_available(&pass_id)?;
                let sender_device_id = user_creds.device_id();
                let vault_name = user_creds.vault_name.clone();

                let p_ss = PersistentSharedSecret::from(self.p_obj.clone());

                let ss_log = p_ss.get_ss_log_obj(vault_name).await?;
                let local_claims: Vec<SsDeviceLogObject> = self
                    .p_obj
                    .get_object_events_from_beginning(SsDeviceLogDescriptor::from(
                        sender_device_id.clone(),
                    ))
                    .await?;
                if let Some(existing) =
                    ownership::select(&ss_log, &local_claims, &user_creds, &pass_id)?
                {
                    receipt.claim_id = Some(existing.claim_id.wasm_id_str());
                    return Ok(if existing.conflicted {
                        receipt.unknown("RecoverCanonicalUnknown")
                    } else {
                        receipt
                    });
                }

                let vault_member = vault_repo
                    .get_vault(member.user().vault_name())
                    .await?
                    .to_data()
                    .to_vault_member(member)?;
                vault_member.vault.ensure_secret_available(&pass_id)?;
                let claim = vault_member.create_recovery_claim(pass_id);
                info!(claim_id = ?claim.id, pass_id = ?claim.dist_claim_id.pass_id, "recovery_request created claim");
                receipt.claim_id = Some(claim.id.wasm_id_str());
                if let Err(error) = p_ss.save_claim_in_ss_device_log(claim).await {
                    if error
                        .is::<crate::node::db::objects::persistent_object::AppendOutcomeUnknown>()
                    {
                        return Ok(receipt.unknown("RecoverPersistenceUnknown"));
                    }
                    anyhow::bail!("RecoverNotQueued");
                }
            }
        }

        Ok(receipt)
    }
}
