//! Privacy-only replacement inside the shared client sync ownership interval.
use super::{PersistentGuardedPreparation, UNAVAILABLE, validation};
use crate::node::db::{
    events::{generic_log_event::ToGenericEvent, local_guarded_preparation::*},
    repo::generic_db::KvLogEventRepo,
};
use anyhow::{Result, ensure};

pub(crate) struct CapturedRetirement {
    index: LocalGuardedPreparationIndexObject,
    object: LocalGuardedPreparationObject,
}

impl<R: KvLogEventRepo> PersistentGuardedPreparation<R> {
    pub(crate) fn capture_retirement(
        &self,
        index: &LocalGuardedPreparationIndexObject,
        object: &LocalGuardedPreparationObject,
    ) -> Result<CapturedRetirement> {
        let value = &object.0.value;
        validation::index(index, &value.owner)?;
        validation::witness(object, &value.owner, &value.request_id)?;
        ensure!(
            index.0.value.request_ids.contains(&value.request_id),
            UNAVAILABLE
        );
        Ok(CapturedRetirement {
            index: index.clone(),
            object: object.clone(),
        })
    }

    /// All witness writers share the caller's sync owner. Reset is not fenced by
    /// that owner: existing-only atomic replace must fail if reset removed this
    /// record. Never fall back to save, upsert or enrollment here.
    pub(crate) async fn redact_retirement(
        &self,
        captured: &CapturedRetirement,
        intended: LocalGuardedPreparation,
    ) -> Result<()> {
        let previous = &captured.object;
        self.capture_retirement(&captured.index, previous)?;
        ensure!(
            previous.0.value.payload.is_some() && intended.payload.is_none(),
            UNAVAILABLE
        );
        ensure!(
            matches!(
                intended.phase,
                GuardedPreparationPhase::BlockedRetiredCommand
                    | GuardedPreparationPhase::RetiredUnqueued
            ),
            UNAVAILABLE
        );
        let mut allowed = previous.0.value.clone();
        allowed.payload = None;
        allowed.phase = intended.phase;
        if let Some(existing) = &allowed.transport_noop {
            ensure!(
                intended.transport_noop.as_ref() == Some(existing),
                UNAVAILABLE
            );
        } else {
            if let Some(candidate) = &intended.transport_noop {
                ensure!(
                    candidate.receipt.is_none() && candidate.confirmed_next_tail.is_none(),
                    UNAVAILABLE
                );
            }
            allowed.transport_noop = intended.transport_noop.clone();
        }
        ensure!(intended == allowed, UNAVAILABLE);
        let object =
            LocalGuardedPreparationObject(crate::node::db::events::kv_log_event::KvLogEvent {
                key: previous.0.key.clone(),
                value: intended,
            });
        validation::witness(&object, &object.0.value.owner, &object.0.value.request_id)?;
        // A FULL preread can fail after auth already became durable. The captured
        // exact candidate survives that uncertainty; only payloadNone is written.
        self.write(object.to_generic(), true).await
    }
}
