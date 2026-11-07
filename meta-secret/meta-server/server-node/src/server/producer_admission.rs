//! Actual verified source admission shares the existing canonical domain checks.
use super::{producer_source::Candidate, server_data_sync::ServerSyncGateway};
use anyhow::{Result, anyhow, ensure};
use meta_secret_core::node::{
    api::{SignedAction, SignedActionPayload},
    common::model::vault::vault_data::VaultData,
    db::{
        actions::vault::{
            automatic_delete_application::AutomaticDeleteAction,
            removal_application::RemovalApplication,
        },
        events::vault::{producer_metadata::*, vault_log_event::*},
        objects::persistent_vault::PersistentVault,
        repo::generic_db::KvLogEventRepo,
    },
};

impl<R: KvLogEventRepo> ServerSyncGateway<R> {
    pub(super) async fn admit_producer_source(
        &self,
        candidate: Candidate,
        action: &SignedAction,
        mut events: VaultActionEvents,
        vault: VaultData,
    ) -> Result<bool> {
        let name = vault.vault_name.clone();
        let p = PersistentVault::from(self.p_obj.clone());
        match &candidate.source.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request)) => {
                // Existing accepted stale/terminal legacy actions retain their
                // existing handling; they acquire no invented historical proof.
                if vault.deletion_results.contains_key(&request.request_id)
                    || vault
                        .deleted_secrets
                        .iter()
                        .any(|pass| pass.id == request.pass_id.id)
                {
                    return Ok(false);
                }
                let mut members: Vec<_> = vault
                    .members()
                    .into_iter()
                    .map(|m| m.user_data.device.device_id)
                    .collect();
                members.sort_by_key(ToString::to_string);
                if request.members != members {
                    return Ok(false);
                }
                vault.ensure_can_delete_secret(request, None)?;
                ensure!(
                    !events
                        .deletion_results
                        .iter()
                        .any(|r| r.request.request_id == request.request_id)
                        && !events.requests.iter().any(
                            |event| matches!(event, VaultActionRequestEvent::DeleteSecret(pending)
                        if pending.pass_id.id == request.pass_id.id
                            && !vault.deletion_results.contains_key(&pending.request_id)
                            && vault.ensure_can_delete_secret(pending, None).is_ok())
                        ),
                    "ProducerMetadata.Unavailable"
                );
                let mode = if request.requires_approval() {
                    ProducerCompletionMode::RemotePending
                } else {
                    ProducerCompletionMode::AutomaticPending
                };
                let record = candidate.record(request.request_id.clone(), Some(mode));
                if request.requires_approval() {
                    events
                        .requests
                        .insert(VaultActionRequestEvent::DeleteSecret(request.clone()));
                    events.add_verified_producer_source(record)?;
                    p.save_vault_log_events(events, name).await?;
                } else {
                    AutomaticDeleteAction {
                        p_obj: self.p_obj.clone(),
                    }
                    .admit(request.clone(), record)
                    .await?;
                }
            }
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request)) => {
                vault.ensure_can_remove_member(request, None)?;
                vault.ensure_valid_removal_transfers(request, &request.share_transfers)?;
                events
                    .requests
                    .insert(VaultActionRequestEvent::RemoveMember(request.clone()));
                events.initialize_removal_bindings()?;
                let binding = events
                    .liveboard_removal_pending
                    .iter()
                    .find(|b| b.request == *request)
                    .ok_or_else(|| anyhow!("ProducerMetadata.Unavailable"))?;
                let record = candidate.record(binding.request_id.clone(), None);
                events.add_verified_producer_source(record)?;
                p.save_vault_log_events(events, name).await?;
            }
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) => {
                if vault.members().len() != 2 {
                    return Ok(false);
                }
                let application = RemovalApplication {
                    p_obj: self.p_obj.clone(),
                };
                application.initialize_pending(name.clone()).await?;
                let current = p.get_vault_log_artifact(name).await?.0.value;
                let Some(binding) = current
                    .liveboard_removal_pending
                    .iter()
                    .find(|b| b.request == update.request)
                else {
                    return Ok(false);
                };
                let record = candidate.record(binding.request_id.clone(), None);
                let proof = if matches!(
                    action.payload,
                    SignedActionPayload::GuardedRemovalDecision(_)
                ) {
                    Some(super::guarded_removal::source_proof(action)?)
                } else {
                    None
                };
                application
                    .apply_with_sources(update, proof, Some(record))
                    .await?;
            }
            _ => return Ok(false),
        }
        self.finish_producer_raw(candidate.source, &candidate.kind)
            .await?;
        Ok(true)
    }
}
