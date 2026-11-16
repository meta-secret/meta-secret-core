//! Every late producer raw save must retain deletion cleanup before settled exposure.
use super::{
    correlation_faults::{Fault, Step},
    correlation_support::observe,
    producer_guard_fixture::{
        GuardFixture, public_reads_before_observation, require_unavailable_or_clean,
    },
    *,
};
use meta_secret_core::node::common::model::liveboard_producer::*;
use meta_secret_core::node::db::repo::generic_db::FindOneQuery;
use std::sync::atomic::Ordering;

async fn acknowledged(admitted: bool) -> Result<()> {
    let h = GuardFixture::new(admitted).await?;
    let original = canonical_json(&h.signed)?;
    send(&h.server, h.signed.clone()).await?;
    h.assert_clean().await?;
    assert_eq!(canonical_json(&h.signed)?, original);
    let reference = h.reference().await?;
    let observed = observe(&h.server, h.seed.actor.user(), reference).await?;
    assert_eq!(observed.device_log_tail, Some(h.source.obj_id().next()));
    let proven = observed
        .producer_observation
        .context("verified original source")?;
    assert_eq!(
        proven.request_stage.observation,
        ProducerSourceObservation::Redacted
    );
    assert_eq!(proven.canonical_state, ProducerCanonicalState::Pending);
    let before = h.retained_ref().await?;
    assert!(
        send(&h.server, h.signed.clone()).await.is_err(),
        "stored old Event stays nonce-rejected without any replay exception"
    );
    assert_eq!(h.retained_ref().await?, before);
    h.send_as(&h.seed.actor, VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(
        meta_secret_core::node::db::events::vault::vault_log_event::AddMetaPassEvent {
            sender: member(&h.seed.actor),
            meta_pass_id: meta_secret_core::node::common::model::meta_pass::MetaPasswordId::build_from_str(
                "legitimate-next-command-after-cleanup",
            ),
        },
    ))).await?;
    Ok(())
}

#[tokio::test]
async fn admitted_remove_raw_gap_then_actual_delete_cannot_restore_ciphertext_on_same_r_ack()
-> Result<()> {
    acknowledged(true).await
}
#[tokio::test]
async fn fresh_remove_source_with_deleted_transfer_is_scrubbed_before_ack_control() -> Result<()> {
    acknowledged(false).await
}

async fn cleanup_fault(fault: Fault) -> Result<()> {
    let h = GuardFixture::new(true).await?;
    let retained = h.retained_ref().await?;
    let id = retained["requestId"].as_str().context("actual P")?;
    h.seed.repo.arm(&h.source, id, Step::Scrub, fault);
    let write = send(&h.server, h.signed.clone()).await;
    // A failed write and a past raw tail do not substitute for cleanup proof.
    require_unavailable_or_clean(&h, &h.server, h.seed.actor.user()).await?;
    if write.is_ok() {
        h.assert_clean().await?;
    }
    assert!(
        h.seed.repo.hits.load(Ordering::SeqCst) > 0,
        "RED: post-raw canonical purge did not reach the actual replacement/readback fault"
    );
    if write.is_err() {
        assert_eq!(
            h.publisher.0.load(Ordering::SeqCst),
            0,
            "no producer invalidation before required cleanup finishes"
        );
    }
    h.seed.repo.clear();
    let reopened = ServerApp::new(h.seed.repo.clone(), h.seed.master.clone())?;
    reopened.init().await?;
    // Ordinary read after reopen must repair/prove cleanup; resending old R is forbidden.
    require_unavailable_or_clean(&h, &reopened, h.seed.actor.user()).await?;
    h.assert_clean().await?;
    assert_eq!(h.retained_ref().await?, retained);
    assert!(send(&reopened, h.signed).await.is_err());
    Ok(())
}
#[tokio::test]
async fn post_raw_cleanup_before_failure_cannot_publish_settled_source() -> Result<()> {
    cleanup_fault(Fault::Before).await
}
#[tokio::test]
async fn post_raw_cleanup_silent_drop_requires_exact_scrub_readback() -> Result<()> {
    cleanup_fault(Fault::Drop).await
}
#[tokio::test]
async fn post_raw_cleanup_readback_failure_cannot_publish_settled_source() -> Result<()> {
    cleanup_fault(Fault::Readback).await
}
#[tokio::test]
async fn post_raw_cleanup_lost_reply_repairs_without_resubmitting_old_event() -> Result<()> {
    cleanup_fault(Fault::LostReply).await
}

async fn raw_uncertainty(fault: Fault) -> Result<()> {
    let h = GuardFixture::new(true).await?;
    let retained = h.retained_ref().await?;
    h.seed.repo.arm(
        &h.source,
        retained["requestId"].as_str().context("actual P")?,
        Step::Raw,
        fault,
    );
    assert!(send(&h.server, h.signed.clone()).await.is_err());
    assert_eq!(h.seed.repo.hits.load(Ordering::SeqCst), 1);
    h.seed.repo.clear();
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), 0);
    // The original body really persisted, so the next ordinary Event is a replay.
    assert!(
        h.seed
            .repo
            .inner
            .find_one(h.source.obj_id())
            .await?
            .is_some()
    );
    let reopened = ServerApp::new(h.seed.repo.clone(), h.seed.master.clone())?;
    reopened.init().await?;
    public_reads_before_observation(&h, &reopened).await?;
    require_unavailable_or_clean(&h, &reopened, h.seed.actor.user()).await?;
    h.assert_clean().await?;
    assert_eq!(h.retained_ref().await?, retained);
    assert!(send(&reopened, h.signed).await.is_err());
    Ok(())
}
#[tokio::test]
async fn raw_save_lost_reply_then_atomic_reopen_observation_cannot_ack_deleted_transfer()
-> Result<()> {
    raw_uncertainty(Fault::LostReply).await
}
#[tokio::test]
async fn raw_save_readback_failure_then_atomic_reopen_observation_cannot_ack_deleted_transfer()
-> Result<()> {
    raw_uncertainty(Fault::Readback).await
}
