//! Read-only operation correlation. A conflicting canonical body cannot close raw ownership.
use crate::node::common::model::{
    meta_pass::MetaPasswordId,
    secret::{ClaimId, RecoveryClientStatus, SecretDistributionType, SsClaim, SsLogData},
    user::user_creds::UserCreds,
};
use crate::node::db::events::shared_secret_event::SsDeviceLogObject;
use anyhow::{Result, ensure};
use std::collections::{HashMap, HashSet};

pub(super) struct Selection {
    pub claim_id: ClaimId,
    pub conflicted: bool,
}

pub(super) fn select(
    canonical: &SsLogData,
    local: &[SsDeviceLogObject],
    owner: &UserCreds,
    pass: &MetaPasswordId,
) -> Result<Option<Selection>> {
    let mut originals: HashMap<ClaimId, Vec<&SsClaim>> = HashMap::new();
    for event in local {
        originals
            .entry(event.0.value.id.clone())
            .or_default()
            .push(&event.0.value);
    }
    let mut candidates: HashMap<ClaimId, bool> = HashMap::new();
    for original in local.iter().map(|event| &event.0.value) {
        if !belongs_to(original, owner, pass) {
            continue;
        }
        ensure!(valid_identity(original), "RecoveryIdentityUnavailable");
        ensure!(
            originals
                .get(&original.id)
                .is_some_and(|group| group.iter().all(|other| same_identity(original, other))),
            "RecoveryIdentityUnavailable"
        );
        let (view, conflicted) = match canonical.claims.get(&original.id) {
            Some(view) if same_identity(original, view) => (view, false),
            Some(_) => (original, true),
            None => (original, false),
        };
        if active(view, owner) {
            candidates
                .entry(original.id.clone())
                .and_modify(|conflict| *conflict |= conflicted)
                .or_insert(conflicted);
        }
    }

    for (key, claim) in &canonical.claims {
        if !belongs_to(claim, owner, pass) {
            continue;
        }
        if let Some(group) = originals.get(key)
            && !group.iter().all(|original| same_identity(original, claim))
        {
            // The real unresolved original is already counted, not overwritten by this body.
            ensure!(
                candidates.get(key) == Some(&true),
                "RecoveryIdentityUnavailable"
            );
            continue;
        }
        ensure!(
            *key == claim.id && valid_identity(claim),
            "RecoveryIdentityUnavailable"
        );
        if active(claim, owner) {
            candidates.entry(key.clone()).or_insert(false);
        }
    }
    ensure!(candidates.len() <= 1, "RecoveryOwnershipAmbiguous");
    Ok(candidates
        .into_iter()
        .next()
        .map(|(claim_id, conflicted)| Selection {
            claim_id,
            conflicted,
        }))
}

fn belongs_to(claim: &SsClaim, owner: &UserCreds, pass: &MetaPasswordId) -> bool {
    claim.sender == *owner.device_id()
        && claim.vault_name == owner.vault_name
        && claim.distribution_type == SecretDistributionType::Recover
        && claim.dist_claim_id.pass_id == *pass
}

fn valid_identity(claim: &SsClaim) -> bool {
    claim.id == claim.dist_claim_id.id
        && claim.receivers.iter().collect::<HashSet<_>>().len() == claim.receivers.len()
}

fn same_identity(left: &SsClaim, right: &SsClaim) -> bool {
    valid_identity(left)
        && valid_identity(right)
        && left.id == right.id
        && left.dist_claim_id == right.dist_claim_id
        && left.sender == right.sender
        && left.vault_name == right.vault_name
        && left.distribution_type == right.distribution_type
        && left.receivers.iter().collect::<HashSet<_>>()
            == right.receivers.iter().collect::<HashSet<_>>()
}

fn active(claim: &SsClaim, owner: &UserCreds) -> bool {
    !matches!(
        claim.compute_client_status(owner.device_id()),
        Some(RecoveryClientStatus::Declined | RecoveryClientStatus::Done)
    )
}
