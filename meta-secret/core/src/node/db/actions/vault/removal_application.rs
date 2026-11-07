//! Bounded repair of one authorized removal. No client command or resharing
//! generation is produced here; the original validated update is replayed.
use crate::node::common::model::secret::SsLogData;
use crate::node::common::model::user::common::{
    UserDataOutsider, UserDataOutsiderStatus, UserMembership,
};
use crate::node::common::model::vault::{
    vault::VaultName,
    vault_data::{VaultAggregate, VaultData},
};
use crate::node::db::descriptors::{
    shared_secret_descriptor::SsLogDescriptor, vault_descriptor::VaultStatusDescriptor,
};
use crate::node::db::events::generic_log_event::{KeyExtractor, ObjIdExtractor};
use crate::node::db::events::object_id::Next;
use crate::node::db::events::vault::{
    liveboard_metadata::*, vault_event::VaultObject, vault_log_event::*,
    vault_status::VaultStatusObject,
};
use crate::node::db::objects::{
    persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
    persistent_vault::PersistentVault,
};
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{Result, ensure};
use std::sync::Arc;

pub struct RemovalApplication<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> RemovalApplication<Repo> {
    pub async fn initialize_pending(&self, name: VaultName) -> Result<()> {
        let p = PersistentVault::from(self.p_obj.clone());
        let Some(log) = p.vault_log(name.clone()).await? else {
            return Ok(());
        };
        let mut events = log.0.value;
        if events.initialize_removal_bindings()? {
            p.save_vault_log_events(events, name).await?;
        }
        Ok(())
    }

    pub async fn apply(&self, update: &RemoveMemberEvent) -> Result<()> {
        self.apply_with_source(update, None).await
    }

    pub async fn apply_with_source(
        &self,
        update: &RemoveMemberEvent,
        source: Option<RemovalSourceProof>,
    ) -> Result<()> {
        self.apply_with_sources(update, source, None).await
    }

    pub async fn apply_with_sources(
        &self,
        update: &RemoveMemberEvent,
        source: Option<RemovalSourceProof>,
        producer: Option<
            crate::node::db::events::vault::producer_metadata::VerifiedProducerSourceRef,
        >,
    ) -> Result<()> {
        let name = update.request.sender.user_data.vault_name();
        self.repair(name.clone()).await?;
        self.initialize_pending(name.clone()).await?;
        let p = PersistentVault::from(self.p_obj.clone());
        let mut events = p.get_vault_log_artifact(name.clone()).await?.0.value;
        events.validate_removal_bindings(true)?;
        let Some(binding) = events
            .liveboard_removal_pending
            .iter()
            .find(|b| b.request == update.request)
            .cloned()
        else {
            // Existing legacy protocol treats late decisions as no-ops.
            return Ok(());
        };
        let base = p.get_vault(name.clone()).await?;
        validate_update(&base.0.value, update)?;
        let target = compute_target(&base, &events, update);
        // The prepared log consumes the pending request. Repair must explicitly
        // restore the ORIGINAL request before applying the frozen update.
        events = target.1;
        if let Some(producer) = producer {
            use crate::node::db::events::vault::producer_metadata::{
                VerifiedProducerKind, VerifiedProducerSubject,
            };
            ensure!(
                base.0.value.members().len() == 2
                    && update.decision == RemoveMemberDecision::Accepted
                    && update.sender == update.request.sender
                    && producer.kind == VerifiedProducerKind::LocalRemoveAccept
                    && producer.request_id == binding.request_id
                    && producer.actor_id == update.sender.user_data.device.device_id
                    && producer.vault_name == name
                    && producer.subject
                        == VerifiedProducerSubject::Device(
                            update.target.user_data.device.device_id.clone()
                        ),
                "ProducerMetadata.Unavailable"
            );
            events.add_verified_producer_source(producer)?;
        }
        events
            .liveboard_removal_pending
            .retain(|b| b.request_id != binding.request_id);
        events.liveboard_removal_results.push(RemovalResultRecord {
            request_id: binding.request_id.clone(),
            requester_id: update.request.sender.user_data.device.device_id.clone(),
            target_id: update.target.user_data.device.device_id.clone(),
            responder_id: update.sender.user_data.device.device_id.clone(),
            outcome: outcome(update.decision),
            application_state: RemovalApplicationState::Prepared,
            source,
        });
        events
            .liveboard_removal_applications
            .push(RemovalApplicationReceipt {
                request_id: binding.request_id,
                base_vault_artifact_id: base.obj_id(),
                target_vault_artifact_id: target.0.obj_id(),
                update: update.clone(),
            });
        events.validate_removal_bindings(true)?;
        p.save_vault_log_events(events, name.clone()).await?;
        self.repair(name).await
    }

    pub async fn repair(&self, name: VaultName) -> Result<()> {
        let p = PersistentVault::from(self.p_obj.clone());
        let Some(log) = p.vault_log(name.clone()).await? else {
            return Ok(());
        };
        let mut events = log.0.value;
        events.validate_removal_bindings(false)?;
        ensure!(
            events.liveboard_removal_applications.len() <= 1,
            "multiple prepared removal transitions"
        );
        let Some(receipt) = events.liveboard_removal_applications.first().cloned() else {
            return Ok(());
        };
        let base: VaultObject = self
            .p_obj
            .repo
            .find_one_obj(receipt.base_vault_artifact_id.clone())
            .await?
            .ok_or_else(|| anyhow::anyhow!("missing removal base record"))?;
        ensure!(
            receipt.target_vault_artifact_id == base.obj_id().next(),
            "invalid removal target sequence"
        );
        validate_update(&base.0.value, &receipt.update)?;
        let result = events
            .liveboard_removal_results
            .iter()
            .find(|r| r.request_id == receipt.request_id)
            .ok_or_else(|| anyhow::anyhow!("missing removal result"))?;
        ensure!(
            result.outcome == outcome(receipt.update.decision),
            "removal receipt outcome mismatch"
        );
        let target = compute_target(&base, &events, &receipt.update).0;
        let head = p.get_vault(name.clone()).await?;
        ensure!(
            head == base || head == target,
            "removal canonical head conflicts with frozen transition"
        );
        self.p_obj.save_verified(target.clone()).await?;
        if receipt.update.decision == RemoveMemberDecision::Accepted {
            self.save_status(&target, &receipt.update).await?;
            self.retire_recoveries(name.clone()).await?;
        }
        Box::pin(
            PersistentSharedSecret::from(self.p_obj.clone()).purge_deleted_secrets(&target.0.value),
        )
        .await?;
        // Cleanup can scrub other pending requests. Reload its canonical record
        // instead of replacing them with pre-cleanup encrypted transfers.
        events = p.get_vault_log_artifact(name.clone()).await?.0.value;
        let result = events
            .liveboard_removal_results
            .iter_mut()
            .find(|r| r.request_id == receipt.request_id)
            .ok_or_else(|| anyhow::anyhow!("missing prepared removal result"))?;
        result.application_state = RemovalApplicationState::Applied;
        events.liveboard_removal_applications.clear();
        events.validate_removal_bindings(true)?;
        p.save_vault_log_events(events, name).await
    }

    async fn save_status(&self, target: &VaultObject, update: &RemoveMemberEvent) -> Result<()> {
        let user = update.new_status.user_data();
        let status = target.0.value.status(user.clone());
        let desc = VaultStatusDescriptor::from(user.user_id());
        if let Some(current) = self.p_obj.find_tail_event(desc.clone()).await?
            && current.0.value == status
        {
            return Ok(());
        }
        let id = self.p_obj.find_free_id_by_obj_desc(desc).await?;
        self.p_obj
            .save_verified(VaultStatusObject::new(status, id))
            .await?;
        Ok(())
    }

    async fn retire_recoveries(&self, name: VaultName) -> Result<()> {
        let Some(log) = self
            .p_obj
            .find_tail_event(SsLogDescriptor::from(name.clone()))
            .await?
        else {
            return Ok(());
        };
        let before: SsLogData = log.0.value;
        let next = before.clone().decline_pending_recovery_claims();
        if next != before {
            let next = PersistentSharedSecret::from(self.p_obj.clone())
                .create_new_ss_log_object(next, name)
                .await?;
            self.p_obj.save_verified(next).await?;
        }
        Ok(())
    }
}

fn outcome(decision: RemoveMemberDecision) -> DecisionOutcome {
    match decision {
        RemoveMemberDecision::Accepted => DecisionOutcome::Accepted,
        RemoveMemberDecision::Declined => DecisionOutcome::Declined,
    }
}

fn compute_target(
    base: &VaultObject,
    events: &VaultActionEvents,
    update: &RemoveMemberEvent,
) -> (VaultObject, VaultActionEvents) {
    let mut original = events.clone();
    original
        .requests
        .insert(VaultActionRequestEvent::RemoveMember(
            update.request.clone(),
        ));
    let mut agg = VaultAggregate::build_from(
        original.apply(VaultActionUpdateEvent::RemoveMember(update.clone())),
        base.0.value.clone(),
    );
    agg.events = agg.events.retire_invalid_deletions(&agg.vault);
    for result in &agg.events.deletion_results {
        agg.vault
            .deletion_results
            .entry(result.request.request_id.clone())
            .or_insert_with(|| result.clone());
    }
    let mut target = base.clone();
    target.0.key = base.key().next();
    target.0.value = agg.vault;
    (target, agg.events)
}

fn validate_update(vault: &VaultData, update: &RemoveMemberEvent) -> Result<()> {
    vault.ensure_can_remove_member(&update.request, Some(&update.sender))?;
    vault.ensure_valid_removal_transfers(&update.request, &update.request.share_transfers)?;
    ensure!(
        update.target == update.request.target,
        "Removal response target must match its request"
    );
    match update.decision {
        RemoveMemberDecision::Accepted => {
            vault.ensure_valid_removal_transfers(&update.request, &update.share_transfers)?;
            ensure!(
                update.target.user_data.device.device_id == update.new_status.device_id()
                    && matches!(
                        update.new_status,
                        UserMembership::Outsider(UserDataOutsider {
                            status: UserDataOutsiderStatus::Declined,
                            ..
                        })
                    ),
                "Accepted removal must remove its target"
            );
        }
        RemoveMemberDecision::Declined => {
            ensure!(
                update.share_transfers.is_empty(),
                "Declined removal must not contain Key Share transfers"
            );
            ensure!(
                update.new_status == UserMembership::Member(update.target.clone()),
                "Declined removal must preserve target membership"
            );
        }
    }
    Ok(())
}
