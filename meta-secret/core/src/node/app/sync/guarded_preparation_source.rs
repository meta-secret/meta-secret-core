//! Compare complete retained preparation with original source identity, without effects.
use crate::node::{
    api::{GuardedRemovalDecision, SignedAction, SignedActionPayload},
    common::model::liveboard_producer::producer_source_commitment,
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor, vault_descriptor::DeviceLogDescriptor,
        },
        events::{
            kv_log_event::{KvKey, KvLogEvent},
            local_guarded_preparation::LocalGuardedPreparation,
            vault::{
                device_log_event::DeviceLogObject,
                vault_log_event::{VaultActionEvent, VaultActionUpdateEvent},
            },
        },
        objects::local_guarded_preparation::{UNAVAILABLE, local_body_commitment},
    },
};
use anyhow::{Result, ensure};

/// The temporary event is only a hash/equality input, never a queued or sent body.
/// Callers retain their actual durable-body or canonical accepted-source proof.
pub(super) fn validate_complete_source(
    value: &LocalGuardedPreparation,
    actual: Option<&DeviceLogObject>,
) -> Result<()> {
    let Some(payload) = &value.payload else {
        // Published payloads stay scrubbed; their existing source/ACK checks remain.
        return Ok(());
    };
    let origin = value
        .origin
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
    let candidate = DeviceLogObject(KvLogEvent {
        key: KvKey::artifact(
            DeviceLogDescriptor::from(value.owner.clone()).to_obj_desc(),
            origin.event_id.clone(),
        ),
        value: VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(
            payload.update.clone(),
        )),
    });
    ensure!(
        producer_source_commitment(&candidate)? == origin.body_commitment,
        UNAVAILABLE
    );
    if let Some(actual) = actual {
        ensure!(candidate == *actual, UNAVAILABLE);
    }
    if let Some(signature) = &origin.business_signature {
        // This digest is linked to the actual canonical source by own_applied.
        // It needs no historical raw body and grants no signing/send authority.
        let action = SignedAction {
            payload: SignedActionPayload::GuardedRemovalDecision(GuardedRemovalDecision {
                version: 1,
                request_id: value.request_id.clone(),
                event: candidate,
            }),
            signer: signature.signer.clone(),
            stream: signature.stream.clone(),
            nonce: signature.nonce,
            signature: signature.signature.clone(),
        };
        ensure!(
            local_body_commitment(&action)? == signature.signed_action_hash,
            UNAVAILABLE
        );
    }
    Ok(())
}
