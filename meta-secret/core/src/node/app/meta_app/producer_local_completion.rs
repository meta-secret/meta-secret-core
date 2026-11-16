//! Exact already-Applied cleanup; never rebuilds a decision or material.
use super::{meta_client_service::MetaClientService, producer_receipt};
use crate::node::{
    app::sync::sync_protocol::SyncProtocol,
    common::model::{liveboard_producer::*, user::user_creds::UserCreds},
    db::{
        events::{
            local_remove_producer::LocalRemoveProducerContext,
            vault::liveboard_metadata::{DecisionOutcome, RemovalApplicationState},
        },
        objects::{
            local_removal_envelope::PersistentRemovalEnvelope, persistent_vault::PersistentVault,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};

impl<R: KvLogEventRepo, S: SyncProtocol> MetaClientService<R, S> {
    pub(super) async fn complete_applied_local_producer(
        &self,
        creds: &UserCreds,
        mut context: LocalRemoveProducerContext,
        mut receipt: LiveboardProducerReceipt,
    ) -> Result<LiveboardProducerReceipt> {
        let p_vault = PersistentVault::from(self.p_obj.clone());
        let vault = p_vault.get_vault(creds.vault_name.clone()).await?;
        let members = vault.0.value.members();
        ensure!(
            members.len() == 1
                && members[0].user_data == creds.user()
                && !vault.0.value.is_member(&context.target),
            "OperationUnavailable"
        );
        let decision = context
            .decision_stage
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OperationUnavailable"))?;
        // The explicit entry validates both exact bodies. Absence never permits
        // reconstruction under the changed singleton membership.
        ensure!(
            self.p_obj
                .repo
                .find_one(decision.source.event_id.clone())
                .await?
                .is_some(),
            "OperationUnavailable"
        );
        let observation = self
            .sync_gateway
            .observe_producer(creds.user(), producer_receipt::reference(&receipt))
            .await?
            .ok_or_else(|| anyhow::anyhow!("OperationUnavailable"))?;
        ensure!(
            observation.request_stage.observation == ProducerSourceObservation::RawAcknowledged
                && observation.decision_stage.as_ref().is_some_and(|s| {
                    s.source == decision.source
                        && s.observation == ProducerSourceObservation::RawAcknowledged
                })
                && observation.canonical_state == ProducerCanonicalState::Applied,
            "OperationUnavailable"
        );
        producer_receipt::apply_observation(&mut receipt, observation)?;
        let request_id = receipt
            .request_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OperationUnavailable"))?;
        let events = p_vault
            .get_vault_log_artifact(creds.vault_name.clone())
            .await?
            .0
            .value;
        events.validate_removal_bindings(true)?;
        let matching: Vec<_> = events
            .liveboard_removal_results
            .iter()
            .filter(|r| r.request_id == *request_id)
            .collect();
        ensure!(
            matches!(matching.as_slice(), [r]
                if r.requester_id == *creds.device_id()
                    && r.responder_id == *creds.device_id()
                    && r.target_id == context.target
                    && r.outcome == DecisionOutcome::Accepted
                    && r.application_state == RemovalApplicationState::Applied),
            "OperationUnavailable"
        );
        // This reads actual local material, even when a failed earlier context
        // save left a stale Pending/Unknown marker. The marker is no authority.
        let readiness = self.producer_readiness(creds, &receipt).await?;
        ensure!(
            ["ready", "done", "settled"].contains(&readiness.as_str())
                && p_vault.get_vault(creds.vault_name.clone()).await? == vault,
            "OperationUnavailable"
        );
        context.request_id = receipt.request_id.clone();
        context.request_stage = receipt.request_stage.clone();
        if let Some(ProducerDecisionStage::Attempted {
            source,
            observation,
        }) = &receipt.decision_stage
        {
            context.decision_stage = Some(ProducerAttemptStage {
                source: source.clone(),
                observation: observation.clone(),
            });
        }
        context.material_stage = ProducerMaterialStage::Ready;
        producer_receipt::apply_context(&mut receipt, &context);
        let registry = PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        };
        if registry.save_producer_context(&context).await.is_err()
            || registry.clear_producer_context(&context).await.is_err()
        {
            receipt.phase = ProducerPhase::Unknown;
            receipt.readiness = "synchronizing".into();
            receipt.error_code = Some("OperationUnavailable".into());
            return Ok(receipt);
        }
        receipt.phase = ProducerPhase::Queued;
        receipt.readiness = readiness;
        receipt.error_code = None;
        Ok(receipt)
    }
}
