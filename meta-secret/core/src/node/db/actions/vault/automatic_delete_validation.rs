//! Exact same-P automatic Delete validation; no historical repair authority.
use crate::node::common::model::vault::{vault::VaultName, vault_data::VaultAggregate};
use crate::node::db::{
    descriptors::{
        object_descriptor::ToObjectDescriptor,
        vault_descriptor::{DeviceLogDescriptor, VaultDescriptor},
    },
    events::{
        generic_log_event::{KeyExtractor, ObjIdExtractor},
        kv_log_event::{KvKey, KvLogEvent},
        object_id::{ArtifactId, Next},
        vault::{
            device_log_event::DeviceLogObject, producer_metadata::*, vault_event::VaultObject,
            vault_log_event::*,
        },
    },
};
use anyhow::{Result, ensure};

pub(super) fn decision(
    events: &VaultActionEvents,
    phase: &AutomaticDeleteApplication,
) -> Result<DeleteSecretDecision> {
    match phase.phase {
        AutomaticDeletePhase::Prepared => {
            let mut matching = events.requests.iter().filter_map(|request| match request {
                VaultActionRequestEvent::DeleteSecret(request)
                    if request.request_id == phase.request_id =>
                {
                    Some(request)
                }
                _ => None,
            });
            let request = matching
                .next()
                .ok_or_else(|| anyhow::anyhow!("AutomaticDelete.Unavailable"))?;
            ensure!(matching.next().is_none(), "AutomaticDelete.Unavailable");
            ensure!(
                !events
                    .deletion_results
                    .iter()
                    .any(|r| r.request.request_id == phase.request_id),
                "AutomaticDelete.Unavailable"
            );
            Ok(DeleteSecretDecision {
                request: request.clone(),
                sender: request.sender.clone(),
                accepted: true,
            })
        }
        AutomaticDeletePhase::PurgePending => {
            let mut matching = events
                .deletion_results
                .iter()
                .filter(|r| r.request.request_id == phase.request_id);
            let result = matching
                .next()
                .ok_or_else(|| anyhow::anyhow!("AutomaticDelete.Unavailable"))?;
            ensure!(
                matching.next().is_none()
                    && result.accepted
                    && result.sender == result.request.sender,
                "AutomaticDelete.Unavailable"
            );
            ensure!(!events.requests.iter().any(|r| matches!(r, VaultActionRequestEvent::DeleteSecret(p) if p.request_id == phase.request_id)), "AutomaticDelete.Unavailable");
            Ok(result.clone())
        }
    }
}

pub(super) fn validate_source(
    events: &VaultActionEvents,
    phase: &AutomaticDeleteApplication,
    result: &DeleteSecretDecision,
    name: &VaultName,
) -> Result<()> {
    let source = events
        .producer_source(&phase.source_event_id)?
        .ok_or_else(|| anyhow::anyhow!("AutomaticDelete.Unavailable"))?;
    ensure!(
        source.kind == VerifiedProducerKind::DeleteRequest
            && source.vault_name == *name
            && source.actor_id == result.request.sender.user_data.device.device_id
            && source.request_id == result.request.request_id
            && source.request_id == phase.request_id
            && source.body_commitment == phase.source_body_commitment
            && source.subject == VerifiedProducerSubject::Secret(result.request.pass_id.id_str())
            && source.completion_mode == Some(ProducerCompletionMode::AutomaticPending),
        "AutomaticDelete.Unavailable"
    );
    let descriptor = DeviceLogDescriptor::from(result.request.sender.user_data.user_id());
    ensure!(
        phase.source_event_id.fqdn == ArtifactId::from(descriptor.clone()).fqdn,
        "AutomaticDelete.Unavailable"
    );
    let original = DeviceLogObject(KvLogEvent {
        key: KvKey::artifact(descriptor.to_obj_desc(), phase.source_event_id.clone()),
        value: VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(
            result.request.clone(),
        )),
    });
    ensure!(
        producer_body_commitment(&original)? == phase.source_body_commitment,
        "AutomaticDelete.Unavailable"
    );
    ensure!(
        source.verified_envelope_digest.len() == 64
            && source
                .verified_envelope_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "AutomaticDelete.Unavailable"
    );
    Ok(())
}

pub(super) fn target(
    base: &VaultObject,
    result: &DeleteSecretDecision,
    name: &VaultName,
) -> Result<VaultObject> {
    ensure!(
        base.0.value.vault_name == *name && !result.request.requires_approval(),
        "AutomaticDelete.Unavailable"
    );
    let descriptor = VaultDescriptor::from(name.clone());
    ensure!(
        base.obj_id().fqdn == ArtifactId::from(descriptor.clone()).fqdn
            && base.0.key.obj_desc == descriptor.to_obj_desc(),
        "AutomaticDelete.Unavailable"
    );
    base.0
        .value
        .ensure_can_delete_secret(&result.request, Some(&result.sender))?;
    ensure!(
        result.accepted && result.sender == result.request.sender,
        "AutomaticDelete.Unavailable"
    );
    let events = VaultActionEvents::default()
        .request(VaultActionRequestEvent::DeleteSecret(
            result.request.clone(),
        ))
        .apply(VaultActionUpdateEvent::DeleteSecret(result.clone()));
    let aggregate = VaultAggregate::build_from(events, base.0.value.clone());
    let mut target = base.clone();
    target.0.key = base.key().next();
    target.0.value = aggregate.vault;
    validate_result(&target, result)?;
    Ok(target)
}

pub(super) fn validate_result(vault: &VaultObject, result: &DeleteSecretDecision) -> Result<()> {
    ensure!(
        vault
            .0
            .value
            .deletion_results
            .get(&result.request.request_id)
            == Some(result)
            && result.accepted
            && vault
                .0
                .value
                .deleted_secrets
                .iter()
                .any(|pass| pass.id == result.request.pass_id.id)
            && !vault
                .0
                .value
                .secrets
                .iter()
                .any(|pass| pass.id == result.request.pass_id.id),
        "AutomaticDelete.Unavailable"
    );
    Ok(())
}

pub(super) fn validate_markers(events: &VaultActionEvents, vault: &VaultObject) -> Result<()> {
    for source in &events.producer_source_refs {
        match source.completion_mode {
            Some(ProducerCompletionMode::AutomaticPending) => {
                ensure!(
                    events
                        .automatic_delete_application
                        .as_ref()
                        .is_some_and(|phase| phase.request_id == source.request_id
                            && phase.source_event_id == source.source_event_id),
                    "AutomaticDelete.Unavailable"
                );
            }
            Some(ProducerCompletionMode::AutomaticComplete) => {
                let result = vault
                    .0
                    .value
                    .deletion_results
                    .get(&source.request_id)
                    .ok_or_else(|| anyhow::anyhow!("AutomaticDelete.Unavailable"))?;
                validate_result(vault, result)?;
                ensure!(
                    source.kind == VerifiedProducerKind::DeleteRequest
                        && source.subject
                            == VerifiedProducerSubject::Secret(result.request.pass_id.id_str())
                        && source.actor_id == result.request.sender.user_data.device.device_id
                        && source.vault_name == vault.0.value.vault_name
                        && events.deletion_results.contains(result),
                    "AutomaticDelete.Unavailable"
                );
            }
            _ => {}
        }
    }
    Ok(())
}
