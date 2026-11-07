//! Local structural consistency only; canonical admission belongs to the gateway.
use super::UNAVAILABLE;
use crate::node::{
    common::model::{secret::SecretDistributionType, user::common::UserId},
    db::{
        descriptors::{
            local_guarded_preparation::*,
            shared_secret_descriptor::SsWorkflowDescriptor,
            vault_descriptor::{DeviceLogDescriptor, VaultDescriptor},
        },
        events::{
            kv_log_event::KvKey, local_guarded_preparation::*,
            shared_secret_event::SsWorkflowObject,
        },
    },
    device_transport_noop::validate_transport_id,
    security::canonical_json,
};
use anyhow::{Result, ensure};
use std::collections::{HashMap, HashSet};

pub(crate) fn local_body_commitment<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(crate::crypto::utils::sha256_hex(&String::from_utf8(
        canonical_json(value)?,
    )?))
}

pub(super) fn index(object: &LocalGuardedPreparationIndexObject, owner: &UserId) -> Result<()> {
    ensure!(
        object.0.key
            == KvKey::from(LocalGuardedPreparationIndexDescriptor {
                owner: owner.clone()
            })
            && object.0.value.version == 1
            && object.0.value.owner == *owner,
        UNAVAILABLE
    );
    let mut ids = HashSet::new();
    for id in &object.0.value.request_ids {
        ensure!(
            crate::node::api::is_liveboard_request_id(id) && ids.insert(id),
            UNAVAILABLE
        );
    }
    Ok(())
}

pub(super) fn witness(
    object: &LocalGuardedPreparationObject,
    owner: &UserId,
    uuid: &str,
) -> Result<()> {
    let x = &object.0.value;
    ensure!(
        object.0.key
            == KvKey::from(LocalGuardedPreparationDescriptor {
                owner: owner.clone(),
                request_id: uuid.into(),
            })
            && x.version == 1
            && x.owner == *owner
            && x.request_id == uuid
            && crate::node::api::is_liveboard_request_id(uuid),
        UNAVAILABLE
    );
    ensure!(
        x.base_vault.fqdn
            == crate::node::db::events::object_id::ArtifactId::from(VaultDescriptor::from(
                owner.vault_name.clone()
            ))
            .fqdn,
        UNAVAILABLE
    );
    validate_transport_id(&x.base_vault).map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
    let mut survivors = HashSet::new();
    for user in &x.survivors {
        ensure!(
            user.vault_name == owner.vault_name && survivors.insert(&user.device.device_id),
            UNAVAILABLE
        );
    }
    ensure!(survivors.contains(&owner.device_id), UNAVAILABLE);
    let mut ids = HashSet::new();
    if let Some(origin) = &x.origin {
        origin_binding(origin, &x.owner, &x.request_id, &x.target_id)?;
        if let Some(payload) = &x.payload {
            ensure!(origin.accepted == (payload.update.decision
                == crate::node::db::events::vault::vault_log_event::RemoveMemberDecision::Accepted), UNAVAILABLE);
        }
        ensure!(ids.insert(&origin.event_id), UNAVAILABLE);
    }
    for attempt in &x.previous_attempts {
        captured_context(&attempt.base_vault, &attempt.survivors, owner)?;
        origin_binding(&attempt.origin, owner, uuid, &attempt.target_id)?;
        ensure!(
            attempt.requester_id != owner.device_id
                && attempt.target_id != owner.device_id
                && attempt.requester_id != attempt.target_id,
            UNAVAILABLE
        );
        let origin = &attempt.origin;
        ensure!(ids.insert(&origin.event_id), UNAVAILABLE);
        ensure!(
            attempt.phase == GuardedPreparationPhase::BlockedRetiredCommand,
            UNAVAILABLE
        );
    }
    if let Some(payload) = &x.payload {
        let update = &payload.update;
        ensure!(
            update.sender.user_data.user_id() == *owner
                && update.request.sender.user_data.device.device_id == x.requester_id
                && update.target.user_data.device.device_id == x.target_id
                && update.request.target == update.target
                && update.request.sender.user_data.vault_name == owner.vault_name,
            UNAVAILABLE
        );
        if update.decision
            == crate::node::db::events::vault::vault_log_event::RemoveMemberDecision::Accepted
        {
            ensure!(!survivors.contains(&x.target_id), UNAVAILABLE);
        } else {
            ensure!(payload.passes.is_empty(), UNAVAILABLE);
        }
        let mut passes = HashSet::new();
        for pass in &payload.passes {
            ensure!(passes.insert(&pass.pass_id), UNAVAILABLE);
            prepared_pass(pass, x)?;
        }
        payload_bindings(payload, x)?;
    }
    match x.phase {
        GuardedPreparationPhase::Staged => ensure!(x.payload.is_some(), UNAVAILABLE),
        GuardedPreparationPhase::Queued
        | GuardedPreparationPhase::Signed
        | GuardedPreparationPhase::AcceptedPromoting => {
            ensure!(x.origin.is_some() && x.payload.is_some(), UNAVAILABLE)
        }
        GuardedPreparationPhase::AcceptedPublished
        | GuardedPreparationPhase::BlockedRetiredCommand => {
            ensure!(x.origin.is_some(), UNAVAILABLE)
        }
        GuardedPreparationPhase::RetiredUnqueued => ensure!(x.payload.is_none(), UNAVAILABLE),
    }
    Ok(())
}

fn payload_bindings(
    payload: &GuardedPreparationPayload,
    x: &LocalGuardedPreparation,
) -> Result<()> {
    let expected: HashSet<_> = payload
        .update
        .request
        .share_transfers
        .iter()
        .filter(|transfer| {
            payload.update.decision
                == crate::node::db::events::vault::vault_log_event::RemoveMemberDecision::Accepted
                && x.survivors.len() == 2
                && transfer.receiver == x.owner.device_id
        })
        .map(|transfer| &transfer.pass_id)
        .collect();
    ensure!(
        payload
            .passes
            .iter()
            .map(|pass| &pass.pass_id)
            .collect::<HashSet<_>>()
            == expected,
        UNAVAILABLE
    );
    let mut claims = HashMap::new();
    for claim in &x.claims {
        ensure!(claims.insert(&claim.pass_id, claim).is_none(), UNAVAILABLE);
    }
    ensure!(claims.len() == payload.passes.len(), UNAVAILABLE);
    let mut packets = HashMap::new();
    for packet in &x.packets {
        ensure!(
            packets.insert(&packet.event_id, packet).is_none(),
            UNAVAILABLE
        );
    }
    let mut payload_packet_ids = HashSet::new();
    for pass in &payload.passes {
        ensure!(
            claims
                .get(&pass.pass_id)
                .is_some_and(|claim| claim.claim == pass.claim),
            UNAVAILABLE
        );
        for packet in &pass.distributions {
            let SsWorkflowObject::Distribution(event) = packet else {
                anyhow::bail!(UNAVAILABLE)
            };
            let receiver = event
                .value
                .secret_message
                .cipher_text()
                .channel
                .receiver()
                .to_device_id();
            let commitment = local_body_commitment(packet)?;
            ensure!(
                payload_packet_ids.insert(&event.key.obj_id)
                    && packets.get(&event.key.obj_id).is_some_and(|compact| {
                        compact.receiver_id == receiver && compact.body_commitment == commitment
                    }),
                UNAVAILABLE
            );
        }
    }
    ensure!(payload_packet_ids.len() == packets.len(), UNAVAILABLE);
    Ok(())
}

fn captured_context(
    base: &crate::node::db::events::object_id::ArtifactId,
    survivors: &[crate::node::common::model::user::common::UserData],
    owner: &UserId,
) -> Result<()> {
    ensure!(
        base.fqdn
            == crate::node::db::events::object_id::ArtifactId::from(VaultDescriptor::from(
                owner.vault_name.clone()
            ))
            .fqdn,
        UNAVAILABLE
    );
    validate_transport_id(base).map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
    let mut ids = HashSet::new();
    for user in survivors {
        ensure!(
            user.vault_name == owner.vault_name && ids.insert(&user.device.device_id),
            UNAVAILABLE
        );
    }
    ensure!(ids.contains(&owner.device_id), UNAVAILABLE);
    Ok(())
}

fn origin_binding(
    origin: &GuardedCommandOrigin,
    owner: &UserId,
    uuid: &str,
    target: &crate::node::common::model::device::common::DeviceId,
) -> Result<()> {
    validate_transport_id(&origin.event_id).map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
    ensure!(
        origin.actor_id == owner.device_id
            && origin.vault_name == owner.vault_name
            && origin.request_id == uuid
            && origin.target_id == *target
            && origin.event_id.fqdn
                == crate::node::db::events::object_id::ArtifactId::from(DeviceLogDescriptor::from(
                    owner.clone()
                ))
                .fqdn
            && origin.body_commitment.len() == 64
            && origin
                .body_commitment
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        UNAVAILABLE
    );
    Ok(())
}

fn prepared_pass(pass: &GuardedPreparedPass, x: &LocalGuardedPreparation) -> Result<()> {
    let claim = &pass.claim;
    ensure!(
        pass.input.claim_id.pass_id == pass.pass_id
            && claim.dist_claim_id.pass_id == pass.pass_id
            && claim.id == claim.dist_claim_id.id
            && claim.vault_name == x.owner.vault_name
            && claim.sender == x.owner.device_id
            && claim.distribution_type == SecretDistributionType::Split,
        UNAVAILABLE
    );
    let input = crate::node::common::model::secret::SsDistributionId {
        pass_id: pass.pass_id.clone(),
        receiver: x.owner.device_id.clone(),
    };
    ensure!(
        pass.input.distribution_id.fqdn
            == crate::node::db::events::object_id::ArtifactId::from(
                SsWorkflowDescriptor::Distribution(input)
            )
            .fqdn
            && pass.input.distribution_commitment.len() == 64,
        UNAVAILABLE
    );
    let expected: HashSet<_> = x.survivors.iter().map(|u| &u.device.device_id).collect();
    let peers: HashSet<_> = expected
        .iter()
        .copied()
        .filter(|id| **id != x.owner.device_id)
        .collect();
    ensure!(
        claim.receivers.iter().collect::<HashSet<_>>() == peers
            && claim.receivers.len() == peers.len(),
        UNAVAILABLE
    );
    let mut recipients = HashSet::new();
    for packet in &pass.distributions {
        let SsWorkflowObject::Distribution(event) = packet else {
            anyhow::bail!(UNAVAILABLE)
        };
        let recipient = event
            .value
            .secret_message
            .cipher_text()
            .channel
            .receiver()
            .to_device_id();
        let id = crate::node::common::model::secret::SsDistributionId {
            pass_id: pass.pass_id.clone(),
            receiver: recipient.clone(),
        };
        ensure!(
            event.key == KvKey::from(SsWorkflowDescriptor::Distribution(id))
                && event.value.vault_name == x.owner.vault_name
                && event.value.claim_id == claim.dist_claim_id
                && expected.contains(&recipient)
                && recipients.insert(recipient),
            UNAVAILABLE
        );
    }
    ensure!(
        recipients.iter().collect::<HashSet<_>>() == expected,
        UNAVAILABLE
    );
    Ok(())
}
