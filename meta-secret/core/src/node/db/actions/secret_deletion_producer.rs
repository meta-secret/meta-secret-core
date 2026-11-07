//! Shared existing Delete builder, before the owning append attempt.
use super::SecretDeletionAction;
use crate::node::{
    common::model::meta_pass::MetaPasswordId,
    db::{
        events::vault::vault_log_event::{DeleteSecretRequest, VaultActionRequestEvent},
        objects::persistent_vault::PersistentVault,
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, bail};

impl<R: KvLogEventRepo> SecretDeletionAction<R> {
    pub(crate) async fn build_request(
        &self,
        pass_id: MetaPasswordId,
    ) -> Result<DeleteSecretRequest> {
        let (vault, sender) = self.membership().await?;
        let events = PersistentVault::from(self.p_obj.clone())
            .get_vault_log_artifact(vault.vault_name.clone())
            .await?
            .0
            .value;
        if events.requests.iter().any(|event| {
            matches!(event,
            VaultActionRequestEvent::DeleteSecret(request) if request.pass_id.id == pass_id.id
                && !vault.deletion_results.contains_key(&request.request_id)
                && vault.ensure_can_delete_secret(request, None).is_ok())
        }) {
            bail!("Deletion is already pending for this Secret");
        }
        let request = DeleteSecretRequest::new(sender, pass_id, &vault);
        vault.ensure_can_delete_secret(&request, None)?;
        Ok(request)
    }
}
