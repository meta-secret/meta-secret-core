//! Safe DTO assembly from actual attempted bodies; never an operation authority.
use crate::node::{
    common::model::liveboard_producer::*,
    db::events::{
        local_remove_producer::LocalRemoveProducerContext,
        vault::{
            device_log_event::DeviceLogObject,
            vault_log_event::{VaultActionEvent, VaultActionRequestEvent},
        },
    },
};
use anyhow::{Result, bail};

pub(super) fn attempted(
    event: &DeviceLogObject,
    stage: ProducerAttemptStage,
    context: Option<&LocalRemoveProducerContext>,
) -> Result<LiveboardProducerReceipt> {
    let (kind, secret_id, target_device_id, request_id) = match &event.0.value {
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(r)) => (
            ProducerKind::DeleteSecret,
            Some(r.pass_id.id_str()),
            None,
            Some(r.request_id.clone()),
        ),
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r)) => (
            ProducerKind::RemoveDevice,
            None,
            Some(r.target.user_data.device.device_id.to_string()),
            None,
        ),
        _ => bail!("OperationUnavailable"),
    };
    let mut receipt = LiveboardProducerReceipt {
        version: LIVEBOARD_PRODUCER_VERSION,
        kind,
        phase: ProducerPhase::Unknown,
        secret_id,
        target_device_id,
        request_id,
        request_stage: stage,
        decision_stage: None,
        material_stage: None,
        canonical_state: ProducerCanonicalState::Unknown,
        readiness: "synchronizing".into(),
        error_code: Some("OperationUnavailable".into()),
    };
    if let Some(c) = context {
        apply_context(&mut receipt, c);
    }
    Ok(receipt)
}

pub(super) fn apply_context(
    receipt: &mut LiveboardProducerReceipt,
    context: &LocalRemoveProducerContext,
) {
    receipt.request_stage = context.request_stage.clone();
    receipt.decision_stage = Some(match &context.decision_stage {
        Some(stage) => ProducerDecisionStage::Attempted {
            source: stage.source.clone(),
            observation: stage.observation.clone(),
        },
        None => ProducerDecisionStage::NotAttempted,
    });
    receipt.material_stage = Some(context.material_stage.clone());
    if context.request_id.is_some() {
        receipt.request_id = context.request_id.clone();
    }
}

pub(super) fn reference(receipt: &LiveboardProducerReceipt) -> ProducerReference {
    ProducerReference {
        version: LIVEBOARD_PRODUCER_VERSION,
        kind: receipt.kind.clone(),
        request_id: receipt.request_id.clone(),
        request_stage: receipt.request_stage.source.clone(),
        decision_stage: match &receipt.decision_stage {
            Some(ProducerDecisionStage::Attempted { source, .. }) => Some(source.clone()),
            _ => None,
        },
    }
}

pub(super) fn apply_observation(
    receipt: &mut LiveboardProducerReceipt,
    observation: ProducerCanonicalObservation,
) -> Result<()> {
    anyhow::ensure!(
        observation.version == LIVEBOARD_PRODUCER_VERSION
            && observation.kind == receipt.kind
            && observation.request_stage.source == receipt.request_stage.source
            && receipt
                .request_id
                .as_ref()
                .is_none_or(|id| observation.request_id.as_ref() == Some(id))
            && receipt
                .secret_id
                .as_ref()
                .is_none_or(|id| observation.secret_id.as_ref() == Some(id))
            && receipt
                .target_device_id
                .as_ref()
                .is_none_or(|id| observation.target_device_id.as_ref() == Some(id)),
        "OperationUnavailable"
    );
    if let (Some(ProducerDecisionStage::Attempted { source, .. }), Some(actual)) =
        (&receipt.decision_stage, &observation.decision_stage)
    {
        anyhow::ensure!(*source == actual.source, "OperationUnavailable");
    }
    receipt.request_id = observation.request_id;
    receipt.secret_id = observation.secret_id;
    receipt.target_device_id = observation.target_device_id;
    receipt.canonical_state = observation.canonical_state;
    receipt.request_stage.observation = observation.request_stage.observation;
    if let (
        Some(ProducerDecisionStage::Attempted {
            observation: local, ..
        }),
        Some(actual),
    ) = (&mut receipt.decision_stage, observation.decision_stage)
    {
        *local = actual.observation;
    }
    Ok(())
}
