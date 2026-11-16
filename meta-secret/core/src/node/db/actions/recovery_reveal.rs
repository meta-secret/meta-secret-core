//! Optional proof of exact currently usable sources. Adds no legacy reveal gate.
use crate::{
    crypto::utils::sha256_hex,
    node::{
        common::model::{
            liveboard_reveal::{LiveboardRevealContext, RevealSourceRef, source_order},
            secret::{
                RecoveryClientStatus, SecretDistributionType, SsClaim, SsDistributionId,
                SsDistributionStatus, SsLogData, SsRecoveryId,
            },
            user::user_creds::UserCreds,
        },
        db::{
            descriptors::{
                object_descriptor::ToObjectDescriptor,
                shared_secret_descriptor::SsWorkflowDescriptor, vault_descriptor::VaultDescriptor,
            },
            events::{
                generic_log_event::ObjIdExtractor, kv_log_event::KvKey,
                shared_secret_event::SsWorkflowObject, vault::vault_event::VaultObject,
            },
        },
        security::canonical_json,
    },
    secret::{
        local_share::{LocalShareProfile, compatible_share_blocks, validate_local_share},
        shared_secret::UserShareDto,
    },
};
use anyhow::{Result, ensure};
use serde::Serialize;
use serde_json::Value;

pub(crate) struct RevealInputs<'a> {
    pub vault: &'a VaultObject,
    pub creds: &'a UserCreds,
    pub claims: &'a SsLogData,
    pub claim: &'a SsClaim,
    pub local: Option<&'a SsWorkflowObject>,
    pub recoveries: &'a [SsWorkflowObject],
    pub evidence: RevealEvidence<'a>,
}

pub(crate) enum RevealEvidence<'a> {
    /// SAME source-aligned Material/handler inputs, in Recovery-then-local order.
    VerifiedDecoded(&'a [UserShareDto]),
    /// Candidate only: exact ciphertext equality to the original successful receipt
    /// proves the old remote inputs. Does not certify fresh remote decryptability.
    DoneSourceContinuity { local: &'a UserShareDto },
}

/// Only public/ciphertext, statically selected source types enter these domains.
fn commitment<T: Serialize>(kind: &str, body: &T) -> Result<String> {
    let canonical = canonical_json(body)?;
    let text = std::str::from_utf8(&canonical)?;
    Ok(sha256_hex(&format!(
        "MetaSecret.LiveboardReveal.{kind}.v1\n{text}"
    )))
}

// Normalize only semantic sets in the reveal digest, never signed/stored Vault data.
fn vault_commitment(vault: &VaultObject) -> Result<String> {
    let mut body = serde_json::to_value(vault)?;
    let value = body
        .get_mut("value")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| anyhow::anyhow!("RevealUnavailable"))?;
    for name in ["secrets", "deletedSecrets"] {
        let entries = value
            .get_mut(name)
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow::anyhow!("RevealUnavailable"))?;
        let mut keyed_entries = entries
            .drain(..)
            .map(|entry| canonical_json(&entry).map(|key| (key, entry)))
            .collect::<Result<Vec<_>>>()?;
        keyed_entries.sort_by(|left, right| left.0.cmp(&right.0));
        *entries = keyed_entries.into_iter().map(|(_, entry)| entry).collect();
    }
    commitment("Vault", &body)
}

pub(crate) fn try_reveal_context(inputs: RevealInputs<'_>) -> Option<LiveboardRevealContext> {
    context(inputs).ok()
}

fn context(i: RevealInputs<'_>) -> Result<LiveboardRevealContext> {
    let vault = &i.vault.0.value;
    let current = i.creds.device_id();
    let pass = &i.claim.dist_claim_id.pass_id;
    vault.ensure_secret_available(pass)?;
    vault.ensure_valid_member_count()?;
    ensure!(
        vault.members().len() == 3 && vault.vault_name == i.creds.vault_name,
        "RevealUnavailable"
    );
    ensure!(
        vault
            .members()
            .iter()
            .any(|member| member.user_data == i.creds.user()),
        "RevealUnavailable"
    );
    let vault_desc = VaultDescriptor::from(i.creds.vault_name.clone()).to_obj_desc();
    ensure!(
        i.vault.0.key.obj_desc == vault_desc && i.vault.obj_id().fqdn == vault_desc.fqdn(),
        "RevealUnavailable"
    );
    ensure!(
        i.claim.distribution_type == SecretDistributionType::Recover
            && i.claim.sender == *current
            && i.claim.vault_name == vault.vault_name
            && i.claim.id == i.claim.dist_claim_id.id
            && i.claims.claims.get(&i.claim.id) == Some(i.claim),
        "RevealUnavailable"
    );
    let status = i.claim.compute_client_status(current);
    match (&i.evidence, &status) {
        (RevealEvidence::VerifiedDecoded(shares), Some(RecoveryClientStatus::Accepted)) => {
            ensure!(shares.len() == i.recoveries.len() + 1, "RevealUnavailable")
        }
        (RevealEvidence::DoneSourceContinuity { .. }, Some(RecoveryClientStatus::Done)) => {}
        _ => anyhow::bail!("RevealUnavailable"),
    }
    let claims = i.claims.clone().with_client_status(current);
    let active = claims.find_unique_active_recovery_claim_id(current, pass)?;
    match status {
        Some(RecoveryClientStatus::Accepted) => ensure!(
            active.as_ref() == Some(&i.claim.id) && !i.claim.status.statuses.contains_key(current),
            "RevealUnavailable"
        ),
        Some(RecoveryClientStatus::Done) => ensure!(
            active.is_none()
                && i.claim.status.statuses.get(current) == Some(&SsDistributionStatus::Delivered),
            "RevealUnavailable"
        ),
        _ => anyhow::bail!("RevealUnavailable"),
    }
    let local = i
        .local
        .ok_or_else(|| anyhow::anyhow!("RevealUnavailable"))?;
    let SsWorkflowObject::Distribution(event) = local else {
        anyhow::bail!("RevealUnavailable")
    };
    let expected = KvKey::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: pass.clone(),
        receiver: current.clone(),
    }));
    ensure!(
        event.key == expected
            && event.value.vault_name == vault.vault_name
            && event.value.claim_id.pass_id == *pass,
        "RevealUnavailable"
    );
    let split = i
        .claims
        .claims
        .get(&event.value.claim_id.id)
        .ok_or_else(|| anyhow::anyhow!("RevealUnavailable"))?;
    let channel = &event.value.secret_message.cipher_text().channel;
    ensure!(
        split.id == split.dist_claim_id.id
            && split.dist_claim_id == event.value.claim_id
            && split.distribution_type == SecretDistributionType::Split
            && split.vault_name == vault.vault_name
            && vault.is_member(&split.sender)
            && channel.sender().to_device_id() == split.sender,
        "RevealUnavailable"
    );
    recipient(&i, &event.value)?;
    let local_share = match i.evidence {
        RevealEvidence::VerifiedDecoded(shares) => &shares[i.recoveries.len()],
        RevealEvidence::DoneSourceContinuity { local } => local,
    };
    validate_local_share(local_share, LocalShareProfile::Exact(vault.sss_cfg()))?;
    let mut shares = vec![local_share];
    let approved = i.claim.approved_recovery_receiver()?;
    ensure!(
        vault.is_member(&approved) && approved != *current && (1..=2).contains(&i.recoveries.len()),
        "RevealUnavailable"
    );
    let mut recovery_sources = Vec::new();
    for (index, packet) in i.recoveries.iter().enumerate() {
        let SsWorkflowObject::Recovery(event) = packet else {
            anyhow::bail!("RevealUnavailable")
        };
        let expected = KvKey::from(SsWorkflowDescriptor::Recovery(SsRecoveryId {
            claim_id: i.claim.dist_claim_id.clone(),
            sender: current.clone(),
            distribution_id: SsDistributionId {
                pass_id: pass.clone(),
                receiver: approved.clone(),
            },
        }));
        let channel = &event.value.secret_message.cipher_text().channel;
        ensure!(
            event.key == expected
                && event.value.claim_id == i.claim.dist_claim_id
                && event.value.vault_name == vault.vault_name
                && channel.sender().to_device_id() == approved,
            "RevealUnavailable"
        );
        recipient(&i, &event.value)?;
        if let RevealEvidence::VerifiedDecoded(decoded) = i.evidence {
            let share = &decoded[index];
            validate_local_share(share, LocalShareProfile::Exact(vault.sss_cfg()))?;
            ensure!(
                shares.iter().all(|prior| {
                    prior.share_id != share.share_id && compatible_share_blocks(prior, share)
                }),
                "RevealUnavailable"
            );
            shares.push(share);
        }
        recovery_sources.push(RevealSourceRef {
            event_id: packet.obj_id(),
            body_commitment: commitment("Recovery", packet)?,
        });
    }
    if matches!(i.evidence, RevealEvidence::VerifiedDecoded(_)) {
        ensure!(
            shares.len() >= vault.sss_cfg().threshold,
            "RevealUnavailable"
        );
    }
    recovery_sources.sort_by(|a, b| source_order(a).cmp(&source_order(b)));
    let result = LiveboardRevealContext {
        version: 1,
        vault_name: vault.vault_name.0.clone(),
        current_device_id: current.to_string(),
        vault_source: RevealSourceRef {
            event_id: i.vault.obj_id(),
            body_commitment: vault_commitment(i.vault)?,
        },
        local_distribution_source: RevealSourceRef {
            event_id: local.obj_id(),
            body_commitment: commitment("Distribution", local)?,
        },
        recovery_sources,
        claim_scope_commitment: scope_commitment(&i)?,
    };
    ensure!(result.is_valid(), "RevealUnavailable");
    Ok(result)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RevealClaimScopeV1 {
    claims: Vec<SsClaim>,
}

fn scope_commitment(i: &RevealInputs<'_>) -> Result<String> {
    let mut claims = Vec::new();
    for (id, claim) in &i.claims.claims {
        if claim.dist_claim_id.pass_id.id != i.claim.dist_claim_id.pass_id.id {
            continue;
        }
        ensure!(
            claim.dist_claim_id.pass_id == i.claim.dist_claim_id.pass_id
                && id == &claim.id
                && id == &claim.dist_claim_id.id
                && claim.vault_name == i.creds.vault_name,
            "RevealUnavailable"
        );
        let mut claim = claim.clone();
        claim.client_status = None;
        if claim.id == i.claim.id {
            claim.status.statuses.remove(i.creds.device_id());
        }
        claims.push(claim);
    }
    claims.sort_by_key(|claim| claim.id.wasm_id_str());
    commitment("ClaimScope", &RevealClaimScopeV1 { claims })
}

fn recipient(
    i: &RevealInputs<'_>,
    data: &crate::node::common::model::secret::SecretDistributionData,
) -> Result<()> {
    let channel = &data.secret_message.cipher_text().channel;
    ensure!(
        channel.receiver().to_device_id() == *i.creds.device_id()
            && *channel.receiver() == i.creds.device().keys.transport_pk,
        "RevealUnavailable"
    );
    Ok(())
}
