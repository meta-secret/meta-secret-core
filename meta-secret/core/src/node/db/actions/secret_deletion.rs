use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::user::common::UserDataMember;
use crate::node::common::model::user::user_creds::UserCreds;
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::events::vault::vault_log_event::{
    DeleteSecretDecision, DeleteSecretRequest, VaultActionEvent, VaultActionRequestEvent,
    VaultActionUpdateEvent,
};
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{Result, anyhow, bail};
use std::sync::Arc;

pub struct SecretDeletionAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub user_creds: UserCreds,
}

impl<Repo: KvLogEventRepo> SecretDeletionAction<Repo> {
    async fn membership(&self) -> Result<(VaultData, UserDataMember)> {
        let vault = PersistentVault::from(self.p_obj.clone())
            .get_vault(self.user_creds.vault_name.clone())
            .await?
            .to_data();
        let member = vault
            .members()
            .into_iter()
            .find(|m| m.user_data.device.device_id == *self.user_creds.device_id())
            .ok_or_else(|| anyhow!("Deletion actor is not a current Vault member"))?;
        Ok((vault, member))
    }

    pub async fn request(&self, pass_id: MetaPasswordId) -> Result<()> {
        let (vault, sender) = self.membership().await?;
        let events = PersistentVault::from(self.p_obj.clone())
            .get_vault_log_artifact(vault.vault_name.clone())
            .await?
            .0
            .value;
        if events.requests.iter().any(|event| matches!(event, VaultActionRequestEvent::DeleteSecret(request) if request.pass_id.id == pass_id.id
            && !vault.deletion_results.contains_key(&request.request_id) && vault.ensure_can_delete_secret(request, None).is_ok())) {
            bail!("Deletion is already pending for this Secret");
        }
        let request = DeleteSecretRequest::new(sender, pass_id, &vault);
        vault.ensure_can_delete_secret(&request, None)?;
        PersistentDeviceLog::from(self.p_obj.clone())
            .save_vault_action(
                &self.user_creds.user(),
                VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request)),
            )
            .await
    }

    pub async fn decide(&self, request_id: &str, accepted: bool) -> Result<()> {
        if uuid::Uuid::parse_str(request_id).is_err() {
            bail!("Invalid deletion request ID");
        }
        let (vault, sender) = self.membership().await?;
        if vault.deletion_results.contains_key(request_id) {
            bail!("Deletion request is already terminal");
        }
        let events = PersistentVault::from(self.p_obj.clone())
            .get_vault_log_artifact(vault.vault_name.clone())
            .await?
            .0
            .value;
        let request = events
            .requests
            .into_iter()
            .find_map(|event| match event {
                VaultActionRequestEvent::DeleteSecret(request)
                    if request.request_id == request_id =>
                {
                    Some(request)
                }
                _ => None,
            })
            .ok_or_else(|| anyhow!("Deletion request is no longer pending"))?;
        vault.ensure_can_delete_secret(&request, Some(&sender))?;
        let decision = DeleteSecretDecision {
            request,
            sender,
            accepted,
        };
        PersistentDeviceLog::from(self.p_obj.clone())
            .save_vault_action(
                &self.user_creds.user(),
                VaultActionEvent::Update(VaultActionUpdateEvent::DeleteSecret(decision)),
            )
            .await
    }
}
