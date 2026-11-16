use super::{
    confirmation_fixture::{self, Echo},
    fixtures::*,
};
use anyhow::{Context, Result};
use meta_secret_core::node::{
    api::SignedActionPayload,
    common::model::{
        liveboard::{LiveboardOperationPhase, LiveboardOperationReceipt},
        meta_pass::MetaPasswordId,
        secret::{SecretDistributionType, SsClaim},
    },
    db::{
        descriptors::shared_secret_descriptor::SsDeviceLogDescriptor,
        events::{
            generic_log_event::GenericKvLogEvent, object_id::ArtifactId,
            shared_secret_event::SsDeviceLogObject,
        },
        objects::persistent_shared_secret::PersistentSharedSecret,
    },
};
use std::{collections::HashMap, sync::atomic::Ordering};

async fn ciphertexts(h: &Harness) -> HashMap<ArtifactId, GenericKvLogEvent> {
    let mut snapshot = h.p.repo.get_db().await;
    snapshot.retain(|_, event| matches!(event, GenericKvLogEvent::SsWorkflow(_)));
    snapshot
}
fn only_same_request(h: &Harness, claim: &SsClaim) -> Result<()> {
    for action in h.protocol.writes.lock().unwrap().iter() {
        action.verify(&h.approver.device().keys.dsa_pk)?;
        let SignedActionPayload::Event(GenericKvLogEvent::SsDeviceLog(event)) = &action.payload
        else {
            anyhow::bail!("checked confirmation sent a completion or unrelated command")
        };
        assert!(
            event.0.value == *claim,
            "only the exact original Recover command may be flushed"
        );
    }
    Ok(())
}

async fn local(h: &Harness) -> Result<Vec<SsClaim>> {
    Ok(h.p
        .get_object_events_from_beginning(SsDeviceLogDescriptor::from(
            h.approver.device_id().clone(),
        ))
        .await?
        .into_iter()
        .map(|e: SsDeviceLogObject| e.to_distribution_request())
        .filter(|c| c.distribution_type == SecretDistributionType::Recover)
        .collect())
}
fn unknown(
    receipt: &LiveboardOperationReceipt,
    claim: &SsClaim,
    pass: &MetaPasswordId,
) -> Result<()> {
    assert_eq!(
        receipt.claim_id.as_deref(),
        Some(claim.id.wasm_id_str().as_str())
    );
    assert_eq!(receipt.secret_id.as_deref(), Some(pass.id_str().as_str()));
    assert_eq!(receipt.phase, LiveboardOperationPhase::Unknown);
    assert_eq!(
        receipt.error_code.as_deref(),
        Some("RecoverCanonicalUnknown")
    );
    let json = serde_json::to_value(receipt)?;
    assert!(
        json.as_object()
            .context("metadata receipt")?
            .keys()
            .all(|k| [
                "kind",
                "secretId",
                "claimId",
                "requestId",
                "phase",
                "errorCode"
            ]
            .contains(&k.as_str()))
    );
    for private in [
        VALUE,
        "PRIVATE-CANONICAL-READ-ERROR",
        "secretMessage",
        "shareBlocks",
        "signature",
    ] {
        assert!(!json.to_string().contains(private));
    }
    Ok(())
}

#[tokio::test]
async fn checked_recovery_exact_canonical_echo_retains_same_actual_id_as_queued() -> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("exact-confirmation").await?;
    let material = ciphertexts(&h).await;
    let service = confirmation_fixture::service(&h, Echo::Exact);
    let receipt = service.recover_checked(pass).await?;
    let claims = local(&h).await?;
    assert_eq!(claims.len(), 1);
    assert_eq!(
        receipt.claim_id.as_deref(),
        Some(claims[0].id.wasm_id_str().as_str())
    );
    assert_eq!(receipt.phase, LiveboardOperationPhase::Queued);
    assert!(receipt.error_code.is_none());
    let canonical = PersistentSharedSecret::from(h.p.clone())
        .get_ss_log_obj(h.vault.vault_name.clone())
        .await?;
    assert_eq!(canonical.claims.get(&claims[0].id), Some(&claims[0]));
    assert_eq!(
        service.sync_gateway.sync.requests.load(Ordering::SeqCst),
        2,
        "confirmation must not add a delivery-consuming SsRequest"
    );
    assert!(ciphertexts(&h).await == material);
    only_same_request(&h, &claims[0])?;
    Ok(())
}

#[tokio::test]
async fn checked_recovery_missing_echo_and_reopen_keep_one_actual_unknown_claim_without_delivery()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("unconfirmed-reopen").await?;
    let material = ciphertexts(&h).await;
    let service = confirmation_fixture::service(&h, Echo::Missing);
    let first = service.recover_checked(pass.clone()).await?;
    let claims = local(&h).await?;
    assert_eq!(claims.len(), 1);
    unknown(&first, &claims[0], &pass)?;
    let canonical = PersistentSharedSecret::from(h.p.clone())
        .get_ss_log_obj(h.vault.vault_name.clone())
        .await?;
    assert!(!canonical.claims.contains_key(&claims[0].id));
    assert_eq!(service.sync_gateway.sync.requests.load(Ordering::SeqCst), 2);
    let before = h.p.repo.get_db().await;
    let reopened = confirmation_fixture::service(&h, Echo::Missing);
    let again = reopened.recover_checked(pass.clone()).await?;
    unknown(&again, &claims[0], &pass)?;
    assert_eq!(
        reopened.sync_gateway.sync.requests.load(Ordering::SeqCst),
        2
    );
    assert_eq!(local(&h).await?, claims);
    assert!(
        h.p.repo.get_db().await == before,
        "Unknown cannot fabricate canonical state or reveal/complete"
    );
    for action in h.protocol.writes.lock().unwrap().iter() {
        assert!(!matches!(
            action.payload,
            SignedActionPayload::RecoveryCompletion(_)
        ));
    }
    assert!(ciphertexts(&h).await == material);
    only_same_request(&h, &claims[0])?;
    Ok(())
}

async fn mismatched_confirmation(mode: Echo) -> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("mismatched-confirmation").await?;
    let material = ciphertexts(&h).await;
    let service = confirmation_fixture::service(&h, mode);
    let receipt = service.recover_checked(pass.clone()).await?;
    let claims = local(&h).await?;
    assert_eq!(claims.len(), 1);
    assert!(
        service.sync_gateway.sync.echoed.lock().unwrap().is_some(),
        "actual controlled mismatch must reach synchronization"
    );
    unknown(&receipt, &claims[0], &pass)?;
    assert_eq!(service.sync_gateway.sync.requests.load(Ordering::SeqCst), 2);
    assert!(ciphertexts(&h).await == material);
    only_same_request(&h, &claims[0])?;
    Ok(())
}

#[tokio::test]
async fn checked_recovery_wrong_canonical_sender_keeps_actual_unknown_id() -> Result<()> {
    mismatched_confirmation(Echo::WrongSender).await
}
#[tokio::test]
async fn checked_recovery_wrong_canonical_vault_keeps_actual_unknown_id() -> Result<()> {
    mismatched_confirmation(Echo::WrongVault).await
}
#[tokio::test]
async fn checked_recovery_wrong_canonical_kind_keeps_actual_unknown_id() -> Result<()> {
    mismatched_confirmation(Echo::WrongKind).await
}
#[tokio::test]
async fn checked_recovery_wrong_canonical_pass_keeps_actual_unknown_id() -> Result<()> {
    mismatched_confirmation(Echo::WrongPass).await
}
#[tokio::test]
async fn checked_recovery_different_canonical_claim_cannot_replace_actual_unknown_id() -> Result<()>
{
    mismatched_confirmation(Echo::WrongId).await
}

#[tokio::test]
async fn checked_recovery_canonical_read_failure_preserves_id_and_returns_only_fixed_unknown_code()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("read-failure-confirmation").await?;
    let material = ciphertexts(&h).await;
    let service = confirmation_fixture::service(&h, Echo::ReadFailure);
    let receipt = service.recover_checked(pass.clone()).await?;
    let claims = local(&h).await?;
    assert_eq!(claims.len(), 1);
    assert_eq!(
        service.p_obj.repo.hits.load(Ordering::SeqCst),
        1,
        "the new post-sync confirmation read must exercise the actual repository fault"
    );
    unknown(&receipt, &claims[0], &pass)?;
    assert_eq!(service.sync_gateway.sync.requests.load(Ordering::SeqCst), 2);
    assert!(ciphertexts(&h).await == material);
    only_same_request(&h, &claims[0])?;
    Ok(())
}

#[tokio::test]
async fn checked_recovery_terminal_canonical_race_is_observed_without_rewriting_or_completing()
-> Result<()> {
    for mode in [Echo::Accepted, Echo::Declined, Echo::Done] {
        let mut h = Harness::new(3).await?;
        let pass = h.seed_secret("terminal-confirmation-race").await?;
        let material = ciphertexts(&h).await;
        let service = confirmation_fixture::service(&h, mode);
        let receipt = service.recover_checked(pass).await?;
        let claims = local(&h).await?;
        assert_eq!(claims.len(), 1);
        assert_eq!(
            receipt.claim_id.as_deref(),
            Some(claims[0].id.wasm_id_str().as_str())
        );
        assert_eq!(receipt.phase, LiveboardOperationPhase::Queued);
        assert!(receipt.error_code.is_none());
        let echoed = service
            .sync_gateway
            .sync
            .echoed
            .lock()
            .unwrap()
            .clone()
            .context("actual canonical view")?;
        let canonical = PersistentSharedSecret::from(h.p.clone())
            .get_ss_log_obj(h.vault.vault_name.clone())
            .await?;
        assert_eq!(canonical.claims.get(&claims[0].id), Some(&echoed));
        assert_eq!(service.sync_gateway.sync.requests.load(Ordering::SeqCst), 2);
        assert!(ciphertexts(&h).await == material);
        only_same_request(&h, &claims[0])?;
    }
    Ok(())
}
