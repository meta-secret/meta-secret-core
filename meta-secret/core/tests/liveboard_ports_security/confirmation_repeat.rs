//! Repeated explicit intent may not replace unresolved durable operation ownership.
use super::{
    confirmation_fixture::{self, Echo},
    fixtures::*,
};
use anyhow::{Context, Result};
use meta_secret_core::node::{
    common::model::{
        liveboard::LiveboardOperationPhase,
        secret::{SecretDistributionType, SsClaim},
    },
    db::{
        descriptors::shared_secret_descriptor::SsDeviceLogDescriptor,
        events::shared_secret_event::SsDeviceLogObject,
        objects::persistent_shared_secret::PersistentSharedSecret,
    },
};

async fn actual_local_claims(h: &Harness) -> Result<Vec<SsClaim>> {
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

async fn repeat_does_not_replace_pending(mode: Echo) -> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h
        .seed_secret("explicit-retry-malformed-confirmation")
        .await?;
    let service = confirmation_fixture::service(&h, mode);
    let first = service.recover_checked(pass.clone()).await?;
    let original = actual_local_claims(&h).await?;
    assert_eq!(
        original.len(),
        1,
        "the first explicit intent creates exactly one actual raw claim"
    );
    assert_eq!(
        first.claim_id.as_deref(),
        Some(original[0].id.wasm_id_str().as_str())
    );
    assert_eq!(first.phase, LiveboardOperationPhase::Unknown);
    assert_eq!(first.error_code.as_deref(), Some("RecoverCanonicalUnknown"));
    let view = PersistentSharedSecret::from(h.p.clone())
        .get_ss_log_obj(h.vault.vault_name.clone())
        .await?;
    let malformed = view
        .claims
        .get(&original[0].id)
        .context("actual synchronized same-ID malformed claim")?;
    assert_eq!(malformed.id, original[0].id);
    match mode {
        Echo::WrongSender => assert_ne!(malformed.sender, original[0].sender),
        Echo::WrongPass => assert_ne!(
            malformed.dist_claim_id.pass_id,
            original[0].dist_claim_id.pass_id
        ),
        _ => anyhow::bail!("this regression is scoped to sender and Pass mismatches"),
    }
    // Second real public service call: it is an explicit retry, not an automatic
    // background command or a new owner/transport mock introduced by this test.
    let second = service.recover_checked(pass).await;
    let after = actual_local_claims(&h).await?;
    let actual_ids: Vec<_> = after.iter().map(|claim| claim.id.wasm_id_str()).collect();
    assert_eq!(
        after.len(),
        1,
        "same-ID malformed confirmation cannot suppress original local ownership and allocate another claim: {actual_ids:?}"
    );
    assert!(
        after[0] == original[0],
        "the original raw body and its status may not be rewritten"
    );
    if let Ok(receipt) = second {
        assert_eq!(receipt.claim_id, first.claim_id);
        assert_eq!(receipt.phase, LiveboardOperationPhase::Unknown);
        assert_eq!(
            receipt.error_code.as_deref(),
            Some("RecoverCanonicalUnknown")
        );
    }
    Ok(())
}

#[tokio::test]
async fn explicit_recover_retry_after_same_id_wrong_sender_never_creates_another_actual_claim()
-> Result<()> {
    repeat_does_not_replace_pending(Echo::WrongSender).await
}

#[tokio::test]
async fn explicit_recover_retry_after_same_id_wrong_pass_never_creates_another_actual_claim()
-> Result<()> {
    repeat_does_not_replace_pending(Echo::WrongPass).await
}
