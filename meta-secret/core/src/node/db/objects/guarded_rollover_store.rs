//! Existing-only installation of one complete explicit attempt after settled N.
use super::{PersistentGuardedPreparation, UNAVAILABLE, validation};
use crate::node::db::{
    events::{generic_log_event::ToGenericEvent, local_guarded_preparation::*},
    repo::generic_db::KvLogEventRepo,
};
use anyhow::{Result, ensure};

impl<R: KvLogEventRepo> PersistentGuardedPreparation<R> {
    /// Caller holds the shared sync owner; no enrollment or insertion is allowed.
    pub(crate) async fn rollover(
        &self,
        index: &LocalGuardedPreparationIndexObject,
        previous: &LocalGuardedPreparationObject,
        mut complete: LocalGuardedPreparation,
    ) -> Result<LocalGuardedPreparationObject> {
        let old = &previous.0.value;
        validation::index(index, &old.owner)?;
        validation::witness(previous, &old.owner, &old.request_id)?;
        ensure!(
            index.0.value.request_ids.contains(&old.request_id)
                && old.payload.is_none()
                && old.phase == GuardedPreparationPhase::BlockedRetiredCommand
                && old.transport_noop.as_ref().is_some_and(|technical| {
                    technical.receipt.is_some() && technical.confirmed_next_tail.is_some()
                })
                && complete.owner == old.owner
                && complete.request_id == old.request_id
                && complete.phase == GuardedPreparationPhase::Staged
                && complete.payload.is_some()
                && complete.origin.is_none()
                && complete.transport_noop.is_none()
                && complete.previous_attempts.is_empty(),
            UNAVAILABLE
        );
        let origin = old
            .origin
            .clone()
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        complete.previous_attempts = old.previous_attempts.clone();
        complete.previous_attempts.push(GuardedCompactAttempt {
            origin,
            transport_noop: old.transport_noop.clone(),
            phase: old.phase,
            base_vault: old.base_vault.clone(),
            requester_id: old.requester_id.clone(),
            target_id: old.target_id.clone(),
            survivors: old.survivors.clone(),
            claims: old.claims.clone(),
            packets: old.packets.clone(),
        });
        let intended =
            LocalGuardedPreparationObject(crate::node::db::events::kv_log_event::KvLogEvent {
                key: previous.0.key.clone(),
                value: complete,
            });
        validation::witness(&intended, &old.owner, &old.request_id)?;
        ensure!(
            self.index(&old.owner).await?.as_ref() == Some(index)
                && self.load(&old.owner, &old.request_id).await?.as_ref() == Some(previous),
            UNAVAILABLE
        );
        let mut changed = index.clone();
        changed.0.value.revision = changed
            .0
            .value
            .revision
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        // Existing-only atomic replacement cannot recreate either record after Reset.
        self.write(changed.to_generic(), true).await?;
        self.replace(previous, intended.0.value).await
    }
}
