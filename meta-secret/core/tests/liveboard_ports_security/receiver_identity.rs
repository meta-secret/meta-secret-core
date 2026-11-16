//! Original receiver identity is immutable; lifecycle statuses are not.
use super::{
    confirmation_fixture::{self, Echo},
    fixtures::*,
};
use anyhow::{Context, Result};
use meta_secret_core::node::{
    common::model::{
        liveboard::{LiveboardOperationPhase, LiveboardOperationReceipt},
        meta_pass::MetaPasswordId,
        secret::{RecoveryClientStatus, SecretDistributionType, SsClaim, SsDistributionStatus},
    },
    db::{
        descriptors::shared_secret_descriptor::SsDeviceLogDescriptor,
        events::shared_secret_event::SsDeviceLogObject,
        objects::persistent_shared_secret::PersistentSharedSecret,
    },
};
use std::{collections::HashSet, sync::atomic::Ordering};

async fn raw(h: &Harness) -> Result<Vec<SsClaim>> {
    Ok(h.p
        .get_object_events_from_beginning(SsDeviceLogDescriptor::from(
            h.approver.device_id().clone(),
        ))
        .await?
        .into_iter()
        .map(|event: SsDeviceLogObject| event.to_distribution_request())
        .filter(|claim| claim.distribution_type == SecretDistributionType::Recover)
        .collect())
}
async fn initial() -> Result<(Harness, MetaPasswordId, SsClaim)> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("immutable-receivers-mutable-status").await?;
    let receipt = h.service().recover_checked(pass.clone()).await?;
    let claims = raw(&h).await?;
    assert_eq!(claims.len(), 1);
    assert_unknown(&receipt, &claims[0]);
    assert_eq!(claims[0].receivers.len(), 2);
    Ok((h, pass, claims[0].clone()))
}
fn assert_unknown(receipt: &LiveboardOperationReceipt, original: &SsClaim) {
    assert_eq!(
        receipt.claim_id.as_deref(),
        Some(original.id.wasm_id_str().as_str())
    );
    assert_eq!(
        receipt.secret_id.as_deref(),
        Some(original.dist_claim_id.pass_id.id_str().as_str())
    );
    assert_eq!(receipt.phase, LiveboardOperationPhase::Unknown);
    assert_eq!(
        receipt.error_code.as_deref(),
        Some("RecoverCanonicalUnknown")
    );
}
async fn canonical(h: &Harness, original: &SsClaim) -> Result<SsClaim> {
    PersistentSharedSecret::from(h.p.clone())
        .get_ss_log_obj(h.vault.vault_name.clone())
        .await?
        .claims
        .get(&original.id)
        .cloned()
        .context("exact canonical claim")
}
async fn save(h: &Harness, claim: SsClaim) -> Result<()> {
    PersistentSharedSecret::from(h.p.clone())
        .save_ss_log_event(claim)
        .await
}

async fn receiver_conflict(empty: bool) -> Result<()> {
    let (h, pass, original) = initial().await?;
    let mut changed = original.clone();
    if empty {
        changed.receivers.clear();
    } else {
        changed.receivers.push(original.receivers[0].clone());
    }
    let mut all_other_fields = changed.clone();
    all_other_fields.receivers = original.receivers.clone();
    assert!(
        all_other_fields == original,
        "only canonical receiver identity is changed"
    );
    if empty {
        assert_eq!(
            changed.compute_client_status(h.approver.device_id()),
            Some(RecoveryClientStatus::Declined),
            "existing Core status helper is terminal-looking for an empty receiver vector"
        );
    } else {
        assert_eq!(
            changed.receivers.iter().collect::<HashSet<_>>(),
            original.receivers.iter().collect::<HashSet<_>>()
        );
        assert_ne!(
            changed.receivers.len(),
            changed.receivers.iter().collect::<HashSet<_>>().len(),
            "set equality alone would conceal the duplicate"
        );
    }
    save(&h, changed.clone()).await?;
    let before = h.p.repo.get_db().await;
    let service = confirmation_fixture::service(&h, Echo::Missing);
    let receipt = service.recover_checked(pass).await?;
    let after = raw(&h).await?;
    let ids: Vec<_> = after.iter().map(|claim| claim.id.wasm_id_str()).collect();
    assert_eq!(
        after.len(),
        1,
        "receiver conflict cannot retire original Pending and allocate another actual claim: {ids:?}"
    );
    assert!(after[0] == original);
    assert_unknown(&receipt, &original);
    assert!(canonical(&h, &original).await? == changed);
    assert!(
        h.p.repo.get_db().await == before,
        "no raw/canonical normalization, status write or packet delivery"
    );
    assert_eq!(
        service.sync_gateway.sync.requests.load(Ordering::SeqCst),
        1,
        "unique conflict must short-circuit after ordinary pre-sync; no extra post-action SsRequest"
    );
    Ok(())
}

#[tokio::test]
async fn empty_receivers_with_other_identity_equal_cannot_close_pending_and_allocate_new_claim()
-> Result<()> {
    receiver_conflict(true).await
}
#[tokio::test]
async fn duplicate_canonical_receiver_even_with_equal_unique_set_is_same_id_unknown() -> Result<()>
{
    receiver_conflict(false).await
}

#[tokio::test]
async fn order_only_receiver_permutation_preserves_valid_reuse_without_normalizing_stored_vectors()
-> Result<()> {
    let (h, pass, original) = initial().await?;
    let mut permuted = original.clone();
    permuted.receivers.reverse();
    assert_ne!(permuted.receivers, original.receivers);
    assert_eq!(
        permuted.receivers.iter().collect::<HashSet<_>>(),
        original.receivers.iter().collect::<HashSet<_>>()
    );
    save(&h, permuted.clone()).await?;
    let before = h.p.repo.get_db().await;
    let receipt = confirmation_fixture::service(&h, Echo::Missing)
        .recover_checked(pass)
        .await?;
    assert_eq!(
        receipt.claim_id.as_deref(),
        Some(original.id.wasm_id_str().as_str())
    );
    assert_eq!(receipt.phase, LiveboardOperationPhase::Queued);
    assert!(receipt.error_code.is_none());
    assert_eq!(raw(&h).await?.len(), 1);
    assert!(canonical(&h, &original).await? == permuted);
    assert!(h.p.repo.get_db().await == before);
    Ok(())
}

async fn completed(h: &Harness, original: &SsClaim) -> Result<SsClaim> {
    let ss = PersistentSharedSecret::from(h.p.clone());
    let view = ss
        .get_ss_log_obj(h.vault.vault_name.clone())
        .await?
        .insert(original.clone())
        .complete_with_receiver_status(
            original.id.clone(),
            original.sender.clone(),
            original.receivers[0].clone(),
            SsDistributionStatus::Sent,
        );
    let done = view
        .claims
        .get(&original.id)
        .cloned()
        .context("actual Core completion result")?;
    assert_eq!(done.receivers, original.receivers);
    assert!(!done.receivers.contains(&done.sender));
    assert_eq!(
        done.status.statuses.get(&done.sender),
        Some(&SsDistributionStatus::Delivered)
    );
    assert_eq!(
        done.compute_client_status(h.approver.device_id()),
        Some(RecoveryClientStatus::Done)
    );
    save(h, done.clone()).await?;
    Ok(done)
}

#[tokio::test]
async fn requester_delivered_outside_receivers_closes_c1_allows_actual_c2_and_then_reuses_c2()
-> Result<()> {
    let (h, pass, original) = initial().await?;
    let done = completed(&h, &original).await?;
    let second = h.service().recover_checked(pass.clone()).await?;
    let claims = raw(&h).await?;
    assert_eq!(
        claims.len(),
        2,
        "genuine Done must allow the next explicit request"
    );
    assert!(claims[0] == original);
    assert_ne!(claims[1].id, original.id);
    assert_unknown(&second, &claims[1]);
    save(&h, claims[1].clone()).await?;
    let before_reuse = h.p.repo.get_db().await;
    let reused = h.service().recover_checked(pass).await?;
    assert_eq!(
        reused.claim_id.as_deref(),
        Some(claims[1].id.wasm_id_str().as_str())
    );
    assert_eq!(reused.phase, LiveboardOperationPhase::Queued);
    assert!(reused.error_code.is_none());
    assert_eq!(raw(&h).await?, claims);
    assert!(canonical(&h, &original).await? == done);
    assert!(h.p.repo.get_db().await == before_reuse);
    Ok(())
}

#[tokio::test]
async fn unresolved_conflicted_c1_and_real_active_c2_fail_closed_instead_of_choosing_later_identity()
-> Result<()> {
    let (h, pass, original) = initial().await?;
    completed(&h, &original).await?;
    h.service().recover_checked(pass.clone()).await?;
    let claims = raw(&h).await?;
    assert_eq!(claims.len(), 2);
    save(&h, claims[1].clone()).await?;
    // Malformed later synchronization destroys proof that C1 actually completed;
    // a later raw position/UUID may not silently supersede the unresolved proof.
    let mut conflicting = original.clone();
    conflicting.sender = original.receivers[0].clone();
    save(&h, conflicting).await?;
    let before = h.p.repo.get_db().await;
    let result = h.service().recover_checked(pass).await;
    assert!(
        result.is_err(),
        "two unresolved identities must fail closed, not return C2 as chosen proof"
    );
    assert_eq!(
        result.err().context("ambiguity error")?.to_string(),
        "RecoverNotQueued"
    );
    assert_eq!(raw(&h).await?, claims);
    assert!(h.p.repo.get_db().await == before);
    Ok(())
}
