//! Durable completion of the SAME admitted automatic Delete; no new command.
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::{
    events::{
        generic_log_event::ObjIdExtractor,
        object_id::Next,
        vault::{producer_metadata::*, vault_event::VaultObject, vault_log_event::*},
    },
    objects::{
        persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
        persistent_vault::PersistentVault,
    },
    repo::generic_db::KvLogEventRepo,
};
use anyhow::{Result, anyhow, ensure};
use std::sync::Arc;

#[path = "automatic_delete_validation.rs"]
mod validation;

pub struct AutomaticDeleteAction<R: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<R>>,
}

impl<R: KvLogEventRepo> AutomaticDeleteAction<R> {
    pub async fn admit(
        &self,
        request: DeleteSecretRequest,
        source: VerifiedProducerSourceRef,
    ) -> Result<()> {
        self.admit_inner(request, source)
            .await
            .map_err(|_| anyhow!("AutomaticDelete.Unavailable"))
    }

    async fn admit_inner(
        &self,
        request: DeleteSecretRequest,
        source: VerifiedProducerSourceRef,
    ) -> Result<()> {
        let name = request.sender.user_data.vault_name();
        let p = PersistentVault::from(self.p_obj.clone());
        let base = p.get_vault(name.clone()).await?;
        base.0
            .value
            .ensure_can_delete_secret(&request, Some(&request.sender))?;
        ensure!(!request.requires_approval(), "AutomaticDelete.Unavailable");
        let mut events = p.get_vault_log_artifact(name.clone()).await?.0.value;
        validation::validate_markers(&events, &base)?;
        ensure!(
            events.automatic_delete_application.is_none(),
            "AutomaticDelete.Unavailable"
        );
        events
            .requests
            .insert(VaultActionRequestEvent::DeleteSecret(request.clone()));
        events.add_verified_producer_source(source.clone())?;
        let phase = AutomaticDeleteApplication {
            request_id: request.request_id.clone(),
            source_event_id: source.source_event_id,
            source_body_commitment: source.body_commitment,
            base_vault_artifact_id: base.obj_id(),
            target_vault_artifact_id: base.obj_id().next(),
            phase: AutomaticDeletePhase::Prepared,
        };
        let result = validation::decision(&events, &phase)?;
        validation::validate_source(&events, &phase, &result, &name)?;
        events.automatic_delete_application = Some(phase);
        p.save_vault_log_events(events, name.clone()).await?;
        self.repair_inner(name).await
    }

    pub async fn repair(&self, name: VaultName) -> Result<()> {
        self.repair_inner(name)
            .await
            .map_err(|_| anyhow!("AutomaticDelete.Unavailable"))
    }

    async fn repair_inner(&self, name: VaultName) -> Result<()> {
        let p = PersistentVault::from(self.p_obj.clone());
        let Some(log) = p.vault_log(name.clone()).await? else {
            return Ok(());
        };
        let mut events = log.0.value;
        let head = p.get_vault(name.clone()).await?;
        validation::validate_markers(&events, &head)?;
        let Some(mut phase) = events.automatic_delete_application.clone() else {
            return Ok(());
        };
        let result = validation::decision(&events, &phase)?;
        validation::validate_source(&events, &phase, &result, &name)?;
        let base: VaultObject = self
            .p_obj
            .repo
            .find_one_obj(phase.base_vault_artifact_id.clone())
            .await?
            .ok_or_else(|| anyhow!("AutomaticDelete.Unavailable"))?;
        let target = validation::target(&base, &result, &name)?;
        ensure!(
            phase.target_vault_artifact_id == target.obj_id(),
            "AutomaticDelete.Unavailable"
        );
        let stored: Option<VaultObject> = self.p_obj.repo.find_one_obj(target.obj_id()).await?;
        if let Some(stored) = &stored {
            ensure!(*stored == target, "AutomaticDelete.Unavailable");
        }
        if head != base && head != target {
            ensure!(
                head.obj_id().fqdn == target.obj_id().fqdn
                    && head.obj_id().id.curr > target.obj_id().id.curr,
                "AutomaticDelete.Unavailable"
            );
            validation::validate_result(&head, &result)?;
            ensure!(stored.is_some(), "AutomaticDelete.Unavailable");
        }
        if phase.phase == AutomaticDeletePhase::Prepared {
            if stored.is_none() {
                ensure!(head == base, "AutomaticDelete.Unavailable");
                self.p_obj.save_verified(target).await?;
            }
            let canonical = p.get_vault(name.clone()).await?;
            validation::validate_result(&canonical, &result)?;
            ensure!(
                events
                    .requests
                    .remove(&VaultActionRequestEvent::DeleteSecret(
                        result.request.clone()
                    )),
                "AutomaticDelete.Unavailable"
            );
            events.deletion_results.insert(result.clone());
            phase.phase = AutomaticDeletePhase::PurgePending;
            events.automatic_delete_application = Some(phase.clone());
            p.save_vault_log_events(events, name.clone()).await?;
        } else {
            ensure!(stored.is_some(), "AutomaticDelete.Unavailable");
            validation::validate_result(&head, &result)?;
        }
        let canonical = p.get_vault(name.clone()).await?;
        Box::pin(
            PersistentSharedSecret::from(self.p_obj.clone())
                .purge_deleted_secrets(&canonical.0.value),
        )
        .await?;
        // Purge may replace logs to scrub retained Remove transfers. Never save
        // the earlier copy, which would resurrect those encrypted packets.
        let mut latest = p.get_vault_log_artifact(name.clone()).await?.0.value;
        ensure!(
            latest.automatic_delete_application.as_ref() == Some(&phase)
                && latest.deletion_results.contains(&result),
            "AutomaticDelete.Unavailable"
        );
        let source = latest
            .producer_source_refs
            .iter_mut()
            .find(|s| s.source_event_id == phase.source_event_id)
            .ok_or_else(|| anyhow!("AutomaticDelete.Unavailable"))?;
        ensure!(
            source.request_id == phase.request_id
                && source.completion_mode == Some(ProducerCompletionMode::AutomaticPending),
            "AutomaticDelete.Unavailable"
        );
        source.completion_mode = Some(ProducerCompletionMode::AutomaticComplete);
        latest.automatic_delete_application = None;
        validation::validate_markers(&latest, &canonical)?;
        p.save_vault_log_events(latest, name).await
    }

    /// Public gateway reads outside the ordered ServerApp may not expose a
    /// partially applied phase. The ordered owner repairs it before reaching here.
    pub async fn ensure_readable(&self, name: VaultName) -> Result<()> {
        let result = async {
            let p = PersistentVault::from(self.p_obj.clone());
            let Some(log) = p.vault_log(name.clone()).await? else {
                return Ok(());
            };
            ensure!(
                log.0.value.automatic_delete_application.is_none(),
                "AutomaticDelete.Unavailable"
            );
            let vault = p.get_vault(name).await?;
            validation::validate_markers(&log.0.value, &vault)
        }
        .await;
        result.map_err(|_| anyhow!("AutomaticDelete.Unavailable"))
    }
}
