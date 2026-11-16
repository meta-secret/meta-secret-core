//! Future-only continuation of an ordinarily verified initial request, not replay.
use super::{
    recovery_intake_validation::{PreparedIntake, digest, initial_source, require, safe},
    server_data_sync::ServerSyncGateway,
    state_invalidation::{StateInvalidation, StateInvalidationScope},
};
use anyhow::Result;
use meta_secret_core::node::{
    api::SignedAction,
    db::{
        descriptors::shared_secret_descriptor::SsLogDescriptor,
        events::{
            generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent},
            kv_log_event::{KvKey, KvLogEvent},
            object_id::Next,
            shared_secret_event::SsLogObject,
            vault::recovery_request_intake::RecoveryRequestIntake,
        },
        objects::{
            persistent_shared_secret::PersistentSharedSecret, persistent_vault::PersistentVault,
        },
        repo::generic_db::KvLogEventRepo,
    },
};

impl<R: KvLogEventRepo> ServerSyncGateway<R> {
    /// Called only after the original Event verification under the ServerApp owner.
    #[tracing::instrument(skip_all)]
    pub(super) async fn try_initial_recovery_intake(
        &self,
        action: &SignedAction,
        event: &GenericKvLogEvent,
    ) -> Result<bool> {
        let GenericKvLogEvent::SsDeviceLog(source) = event else {
            return Ok(false);
        };
        let claim = &source.0.value;
        let vault = safe(
            PersistentVault::from(self.p_obj.clone())
                .get_vault(claim.vault_name.clone())
                .await,
        )?;
        if !initial_source(&vault.0.value, source) {
            return Ok(false);
        }
        let shared = PersistentSharedSecret::from(self.p_obj.clone());
        let base = safe(
            shared
                .find_ss_log_tail_event(claim.vault_name.clone())
                .await,
        )?;
        let mut state = base.as_ref().map(|r| r.0.value.clone()).unwrap_or_default();
        if state.claims.contains_key(&claim.id) {
            return Ok(false);
        }
        if safe(
            state
                .clone()
                .with_client_status(&claim.sender)
                .find_unique_active_recovery_claim_id(&claim.sender, &claim.dist_claim_id.pass_id),
        )?
        .is_some()
        {
            return Ok(false);
        }
        require(
            state.pending_recovery_request_intake.is_none()
                && !state.liveboard_recovery_results.contains_key(&claim.id),
        )?;
        let descriptor = SsLogDescriptor::from(claim.vault_name.clone());
        let key = base
            .as_ref()
            .map(|r| {
                let mut key = r.0.key.clone();
                key.obj_id = r.obj_id().next();
                key
            })
            .unwrap_or_else(|| KvKey::from(descriptor));
        let receipt = RecoveryRequestIntake {
            version: 1,
            claim_id: claim.id.clone(),
            requester_id: claim.sender.clone(),
            source_key: source.0.key.clone(),
            source_body_hash: digest(source)?,
            verified_action_hash: digest(action)?,
            verified_vault_artifact_id: vault.obj_id(),
            base_ss_log_artifact_id: base.as_ref().map(ObjIdExtractor::obj_id),
            target_ss_log_artifact_id: key.obj_id.clone(),
        };
        state.claims.insert(claim.id.clone(), claim.clone());
        state.pending_recovery_request_intake = Some(receipt);
        let record = SsLogObject(KvLogEvent { key, value: state });
        safe(self.p_obj.save_verified(record.clone()).await)?;
        self.finish_initial_recovery_intake(PreparedIntake {
            record,
            source: source.clone(),
        })
        .await?;
        Ok(true)
    }

    #[tracing::instrument(skip_all)]
    pub(super) async fn finish_initial_recovery_intake(
        &self,
        prepared: PreparedIntake,
    ) -> Result<()> {
        let PreparedIntake { record, source } = prepared;
        let name = source.0.value.vault_name.clone();
        require(
            safe(self.p_obj.repo.find_one(record.obj_id()).await)?
                == Some(record.clone().to_generic()),
        )?;
        match safe(self.p_obj.repo.find_one(source.obj_id()).await)? {
            Some(raw) => require(raw == source.clone().to_generic())?,
            None => {
                safe(self.p_obj.save_verified(source).await)?;
            }
        }
        let mut clean = record;
        clean.0.value.pending_recovery_request_intake = None;
        let key = safe(self.p_obj.repo.replace(clean.clone()).await)?;
        require(
            key == clean.obj_id()
                && safe(self.p_obj.repo.find_one(key).await)? == Some(clean.clone().to_generic()),
        )?;
        self.invalidation_publisher.publish(StateInvalidation::new(
            name,
            StateInvalidationScope::SsClaims,
        ));
        Ok(())
    }
}
