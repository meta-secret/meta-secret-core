//! A fixed packet key alone never identifies an owned preparation generation.
use crate::node::{
    common::model::{
        secret::{SecretDistributionData, SecretDistributionType, SsDistributionId},
        user::common::UserData,
    },
    db::{
        descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
        events::{
            generic_log_event::KeyExtractor,
            kv_log_event::KvKey,
            local_guarded_preparation::{
                GuardedClaimOwnership, GuardedPacketOwnership, GuardedPreparationPhase,
                LocalGuardedPreparationObject,
            },
            shared_secret_event::SsWorkflowObject,
        },
        objects::local_guarded_preparation::{UNAVAILABLE, local_body_commitment},
    },
    device_transport_noop::is_transport_digest,
};
use anyhow::{Result, ensure};
use std::collections::HashSet;

pub(super) struct Candidate<'a> {
    pub object: &'a LocalGuardedPreparationObject,
    pub packet: &'a GuardedPacketOwnership,
    claim: &'a GuardedClaimOwnership,
    pub exact_incoming: bool,
}

struct Attempt<'a> {
    current: Option<&'a LocalGuardedPreparationObject>,
    survivors: &'a [UserData],
    claims: &'a [GuardedClaimOwnership],
    packets: &'a [GuardedPacketOwnership],
    published: bool,
    accepted: bool,
}

/// Finish the complete current/history scan before any successful import/retention.
pub(super) fn candidates<'a>(
    records: &'a [LocalGuardedPreparationObject],
    user: &UserData,
    incoming: &SsWorkflowObject,
) -> Result<Vec<Candidate<'a>>> {
    let data = addressed_packet(user, incoming)?;
    let commitment = local_body_commitment(incoming)?;
    let mut candidates = vec![];
    for object in records {
        let value = &object.0.value;
        collect(
            Attempt {
                current: Some(object),
                survivors: &value.survivors,
                claims: &value.claims,
                packets: &value.packets,
                published: matches!(
                    value.phase,
                    GuardedPreparationPhase::AcceptedPromoting
                        | GuardedPreparationPhase::AcceptedPublished
                ),
                accepted: value.origin.as_ref().is_none_or(|origin| origin.accepted),
            },
            user,
            data,
            &commitment,
            &mut candidates,
        )?;
        for previous in &value.previous_attempts {
            collect(
                Attempt {
                    current: None,
                    survivors: &previous.survivors,
                    claims: &previous.claims,
                    packets: &previous.packets,
                    published: false,
                    accepted: previous.origin.accepted,
                },
                user,
                data,
                &commitment,
                &mut candidates,
            )?;
        }
    }
    ensure!(
        candidates.iter().filter(|c| c.exact_incoming).count() <= 1,
        UNAVAILABLE
    );
    Ok(candidates)
}

fn collect<'a>(
    attempt: Attempt<'a>,
    user: &UserData,
    incoming: &SecretDistributionData,
    commitment: &str,
    candidates: &mut Vec<Candidate<'a>>,
) -> Result<()> {
    let expected = KvKey::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: incoming.claim_id.pass_id.clone(),
        receiver: user.device.device_id.clone(),
    }));
    let claims: Vec<_> = attempt
        .claims
        .iter()
        .filter(|c| c.pass_id == incoming.claim_id.pass_id || c.claim.id == incoming.claim_id.id)
        .collect();
    let packets: Vec<_> = attempt
        .packets
        .iter()
        .filter(|p| p.event_id == expected.obj_id)
        .collect();
    if claims.is_empty() && packets.is_empty() {
        return Ok(());
    }
    ensure!(claims.len() == 1 && packets.len() == 1, UNAVAILABLE);
    let owned = claims[0];
    let claim = &owned.claim;
    let packet = packets[0];
    let peers: HashSet<_> = attempt
        .survivors
        .iter()
        .map(|u| &u.device.device_id)
        .filter(|id| **id != user.device.device_id)
        .collect();
    ensure!(
        attempt.accepted
            && attempt.survivors.len() == 2
            && attempt.survivors.contains(user)
            && owned.pass_id == incoming.claim_id.pass_id
            && claim.id == claim.dist_claim_id.id
            && claim.dist_claim_id.pass_id == owned.pass_id
            && claim.vault_name == user.vault_name
            && claim.sender == user.device.device_id
            && claim.distribution_type == SecretDistributionType::Split
            && claim.receivers.len() == peers.len()
            && claim.receivers.iter().collect::<HashSet<_>>() == peers
            && packet.receiver_id == user.device.device_id
            && is_transport_digest(&packet.body_commitment),
        UNAVAILABLE
    );
    let exact_incoming = incoming.claim_id == claim.dist_claim_id;
    ensure!(
        (commitment == packet.body_commitment) == exact_incoming,
        UNAVAILABLE
    );
    if exact_incoming {
        ensure!(
            attempt.published
                && incoming.secret_message.cipher_text().channel.sender()
                    == &user.device.keys.transport_pk,
            UNAVAILABLE
        );
    }
    if attempt.published {
        candidates.push(Candidate {
            object: attempt
                .current
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?,
            packet,
            claim: owned,
            exact_incoming,
        });
    }
    Ok(())
}

fn addressed_packet<'a>(
    user: &UserData,
    packet: &'a SsWorkflowObject,
) -> Result<&'a SecretDistributionData> {
    let SsWorkflowObject::Distribution(event) = packet else {
        anyhow::bail!(UNAVAILABLE)
    };
    let expected = KvKey::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: event.value.claim_id.pass_id.clone(),
        receiver: user.device.device_id.clone(),
    }));
    ensure!(
        packet.key() == expected
            && event.value.vault_name == user.vault_name
            && event.value.secret_message.cipher_text().channel.receiver()
                == &user.device.keys.transport_pk,
        UNAVAILABLE
    );
    Ok(&event.value)
}

pub(super) fn validate_owned_packet(
    user: &UserData,
    candidate: &Candidate<'_>,
    packet: &SsWorkflowObject,
) -> Result<()> {
    let data = addressed_packet(user, packet)?;
    ensure!(
        data.claim_id == candidate.claim.claim.dist_claim_id
            && data.secret_message.cipher_text().channel.sender() == &user.device.keys.transport_pk,
        UNAVAILABLE
    );
    Ok(())
}
