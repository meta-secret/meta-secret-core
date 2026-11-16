//! One durable server-private first Recovery response, never an event-log packet copy.
use super::{
    server_data_sync::ServerSyncGateway,
    state_invalidation::{RecoveryDecisionOutcome, StateInvalidation, StateInvalidationScope},
};
use anyhow::Result;
use meta_secret_core::node::{
    api::SignedAction,
    common::model::{
        secret::{SsDistributionStatus, SsLogData},
        vault::vault::VaultName,
    },
    db::{
        events::{
            generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent},
            object_id::Next,
            shared_secret_event::SsLogObject,
            vault::liveboard_metadata::{DecisionOutcome, RecoveryDecisionRecord},
        },
        objects::persistent_shared_secret::PersistentSharedSecret,
        repo::{
            generic_db::KvLogEventRepo,
            recovery_response_store::*,
            recovery_response_validation::{private_store_error, recovery_source_proof},
        },
    },
};

impl<R: KvLogEventRepo> ServerSyncGateway<R> {
    pub(super) async fn try_apply_recovery_response(
        &self,
        action: &SignedAction,
        event: &GenericKvLogEvent,
    ) -> Result<bool> {
        let Some((name, claim_id, outcome)) = super::recovery_response::response_identity(event)
        else {
            return Ok(false);
        };
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        let base = p_ss
            .find_ss_log_tail_event(name.clone())
            .await?
            .ok_or(RecoveryResponseStoreError::Conflict)?;
        let source = recovery_source_proof(action, outcome)?;
        if let Some(existing) = base
            .0
            .value
            .liveboard_recovery_results
            .get(&claim_id)
            .and_then(|r| r.source.as_ref())
        {
            if *existing == source {
                return Ok(true);
            }
            if existing.workflow_artifact_id == source.workflow_artifact_id {
                return Err(RecoveryResponseStoreError::Conflict.into());
            }
        }
        let claim = base
            .0
            .value
            .claims
            .get(&claim_id)
            .ok_or(RecoveryResponseStoreError::Conflict)?;
        // Late/historical behavior remains in the existing processor. Only a
        // canonical all-Pending addressed first response receives new proof.
        if claim.status.get(&action.signer) != Some(&SsDistributionStatus::Pending)
            || claim
                .status
                .statuses
                .values()
                .any(|s| *s != SsDistributionStatus::Pending)
        {
            return Ok(false);
        }
        let row = PrivateRecoveryResponse {
            version: 1,
            vault_name: name.clone(),
            claim_id,
            base_ss_log_artifact_id: base.obj_id(),
            target_ss_log_artifact_id: base.obj_id().next(),
            source_proof: source,
            phase: RecoveryResponsePhase::Prepared,
            packet: Some(action.clone()),
        };
        let store = self.p_obj.repo.server_recovery_response_store()?;
        store
            .reserve(&name, &row)
            .await
            .map_err(private_store_error)?;
        if store.load(&name).await.map_err(private_store_error)? != Some(row) {
            return Err(RecoveryResponseStoreError::Unavailable.into());
        }
        self.repair_recovery_response(&name).await?;
        Ok(true)
    }
    pub(super) async fn repair_recovery_response(&self, name: &VaultName) -> Result<()> {
        let store = self.p_obj.repo.server_recovery_response_store()?;
        let Some(row) = store.load(name).await.map_err(private_store_error)? else {
            return Ok(());
        };
        row.validate(name)?;
        let committed = self.canonical_recovery_commit(&row).await?;
        if !committed {
            if row.phase != RecoveryResponsePhase::Prepared {
                return Err(RecoveryResponseStoreError::Conflict.into());
            }
            let packet = row
                .packet
                .as_ref()
                .ok_or(RecoveryResponseStoreError::CorruptRecord)?;
            // Recheck normal signature/current actor/claim/stream. Only this exact
            // private reservation may finish an already saved raw source nonce.
            let event = self
                .verify_event_action(packet)
                .await
                .map_err(private_store_error)?;
            let target = self.recovery_target(&row).await?;
            self.save_recovery_fixed(event).await?;
            self.save_recovery_fixed(target.to_generic()).await?;
        }
        store
            .mark_committed(name, &row.source_proof)
            .await
            .map_err(private_store_error)?;
        let expected = row.to_committed(name, &row.source_proof)?;
        if store.load(name).await.map_err(private_store_error)? != Some(expected) {
            return Err(RecoveryResponseStoreError::Unavailable.into());
        }
        store
            .try_remove_committed(name, &row.source_proof)
            .await
            .map_err(private_store_error)?;
        if store
            .load(name)
            .await
            .map_err(private_store_error)?
            .is_some()
        {
            return Err(RecoveryResponseStoreError::Unavailable.into());
        }
        self.invalidation_publisher.publish(StateInvalidation::new(
            name.clone(),
            StateInvalidationScope::SsClaims,
        ));
        self.invalidation_publisher
            .publish(StateInvalidation::recovery_decision(
                name.clone(),
                row.claim_id.wasm_id_str(),
                match row.source_proof.outcome {
                    DecisionOutcome::Accepted => RecoveryDecisionOutcome::Accepted,
                    DecisionOutcome::Declined => RecoveryDecisionOutcome::Declined,
                },
            ));
        Ok(())
    }
    async fn canonical_recovery_commit(&self, row: &PrivateRecoveryResponse) -> Result<bool> {
        let saved = self
            .p_obj
            .repo
            .find_one(row.target_ss_log_artifact_id.clone())
            .await
            .map_err(private_store_error)?;
        let Some(GenericKvLogEvent::SsLog(target)) = saved else {
            if saved.is_some() {
                return Err(RecoveryResponseStoreError::Conflict.into());
            }
            return Ok(false);
        };
        let record = target
            .0
            .value
            .liveboard_recovery_results
            .get(&row.claim_id)
            .ok_or(RecoveryResponseStoreError::Conflict)?;
        if record.source.as_ref() != Some(&row.source_proof)
            || record.claim_id != row.claim_id
            || record.responder_id != row.source_proof.responder_id
            || record.outcome != row.source_proof.outcome
        {
            return Err(RecoveryResponseStoreError::Conflict.into());
        }
        if row.phase == RecoveryResponsePhase::Prepared
            && target != self.recovery_target(row).await?
        {
            return Err(RecoveryResponseStoreError::Conflict.into());
        }
        Ok(true)
    }
    async fn recovery_target(&self, row: &PrivateRecoveryResponse) -> Result<SsLogObject> {
        let Some(GenericKvLogEvent::SsLog(mut base)) = self
            .p_obj
            .repo
            .find_one(row.base_ss_log_artifact_id.clone())
            .await
            .map_err(private_store_error)?
        else {
            return Err(RecoveryResponseStoreError::Conflict.into());
        };
        let claim = base
            .0
            .value
            .claims
            .get(&row.claim_id)
            .ok_or(RecoveryResponseStoreError::Conflict)?;
        if claim.vault_name != row.vault_name
            || claim.status.get(&row.source_proof.responder_id)
                != Some(&SsDistributionStatus::Pending)
            || claim
                .status
                .statuses
                .values()
                .any(|s| *s != SsDistributionStatus::Pending)
            || base
                .0
                .value
                .liveboard_recovery_results
                .contains_key(&row.claim_id)
        {
            return Err(RecoveryResponseStoreError::Conflict.into());
        }
        base.0.value = apply_first(base.0.value, row);
        base.0.key.obj_id = row.target_ss_log_artifact_id.clone();
        Ok(base)
    }
    async fn save_recovery_fixed(&self, event: GenericKvLogEvent) -> Result<()> {
        let id = event.obj_id();
        if let Some(existing) = self
            .p_obj
            .repo
            .find_one(id.clone())
            .await
            .map_err(private_store_error)?
        {
            if existing != event {
                return Err(RecoveryResponseStoreError::Conflict.into());
            }
            return Ok(());
        }
        let saved = self
            .p_obj
            .repo
            .save(event.clone())
            .await
            .map_err(private_store_error)?;
        if saved != id
            || self
                .p_obj
                .repo
                .find_one(id)
                .await
                .map_err(private_store_error)?
                != Some(event)
        {
            return Err(RecoveryResponseStoreError::Unavailable.into());
        }
        Ok(())
    }
    pub(super) async fn exact_recovery_replay(
        &self,
        action: &SignedAction,
        event: &GenericKvLogEvent,
    ) -> Result<bool> {
        let Some((name, claim, outcome)) = super::recovery_response::response_identity(event)
        else {
            return Ok(false);
        };
        let proof = recovery_source_proof(action, outcome)?;
        let log = PersistentSharedSecret::from(self.p_obj.clone())
            .get_ss_log_obj(name.clone())
            .await?;
        if log
            .liveboard_recovery_results
            .get(&claim)
            .and_then(|r| r.source.as_ref())
            == Some(&proof)
        {
            return Ok(true);
        }
        let row = self
            .p_obj
            .repo
            .server_recovery_response_store()?
            .load(&name)
            .await
            .map_err(private_store_error)?;
        if let Some(row) = row {
            row.validate(&name)?;
            return Ok(row.phase == RecoveryResponsePhase::Prepared
                && row.source_proof == proof
                && row.packet.as_ref() == Some(action));
        }
        Ok(false)
    }
}
fn apply_first(mut log: SsLogData, row: &PrivateRecoveryResponse) -> SsLogData {
    log.liveboard_recovery_results.insert(
        row.claim_id.clone(),
        RecoveryDecisionRecord {
            claim_id: row.claim_id.clone(),
            responder_id: row.source_proof.responder_id.clone(),
            outcome: row.source_proof.outcome,
            source: Some(row.source_proof.clone()),
        },
    );
    match row.source_proof.outcome {
        DecisionOutcome::Accepted => {
            log.sent(row.claim_id.clone(), row.source_proof.responder_id.clone())
        }
        DecisionOutcome::Declined => log.decline_remaining_pending(row.claim_id.clone()),
    }
}
