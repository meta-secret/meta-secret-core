use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::api::SignedActionPayload;
use meta_secret_core::node::db::events::{
    generic_log_event::ObjIdExtractor, local_removal_envelope::RemovalEnvelopeMode,
};
use meta_secret_core::node::db::objects::{
    local_removal_envelope::PersistentRemovalEnvelope, persistent_device_log::PersistentDeviceLog,
};
use meta_secret_core::node::db::repo::generic_db::{DbCleanUpCommand, SaveCommand};
use std::sync::atomic::Ordering;

#[tokio::test]
async fn outbox_ordinary_flush_and_reopen_use_the_exact_guarded_envelope() -> Result<()> {
    let h = Harness::new().await?;
    let event = h.checked_decline().await?;
    h.protocol.fail_next_write.store(true, Ordering::SeqCst);
    assert!(h.gateway().sync(h.actor.user()).await.is_err());
    assert_eq!(h.writes().len(), 1);
    assert_guarded(&h.writes()[0], &event, &h.actor)?;
    h.reopened().sync(h.actor.user()).await?;
    let writes = h.writes();
    assert_eq!(writes.len(), 2);
    assert_guarded(&writes[1], &event, &h.actor)?;
    assert!(
        writes[0] == writes[1],
        "restart must retain signature body/ref/nonce for the same pending item"
    );
    Ok(())
}

#[tokio::test]
async fn outbox_managed_marker_missing_fails_closed_in_selector_and_ordinary_flush() -> Result<()> {
    let h = Harness::new().await?;
    let event = h.install(RemovalEnvelopeMode::Guarded).await?;
    let registry = PersistentRemovalEnvelope {
        p_obj: h.p_obj.clone(),
    }
    .load(&h.actor.user_id())
    .await?
    .context("registry")?;
    let mut damaged = registry;
    damaged.0.value.entries.clear(); // Durable managed boundary remains.
    h.p_obj.repo.replace(damaged).await?;
    assert!(
        h.gateway()
            .sign_queued_device_event(event, &h.actor.user())
            .await
            .is_err()
    );
    assert!(
        h.reopened().sync(h.actor.user()).await.is_err(),
        "RED: missing ownership at managed boundary cannot become legacy"
    );
    assert!(
        h.writes().is_empty(),
        "corrupt/missing ownership must be checked before signing/send"
    );
    Ok(())
}

#[tokio::test]
async fn outbox_marker_id_or_ref_corruption_is_rejected_before_any_legacy_send() -> Result<()> {
    let h = Harness::new().await?;
    h.install(RemovalEnvelopeMode::Guarded).await?;
    let mut registry = PersistentRemovalEnvelope {
        p_obj: h.p_obj.clone(),
    }
    .load(&h.actor.user_id())
    .await?
    .context("registry")?;
    registry.0.value.entries[0].request_id = None;
    h.p_obj.repo.replace(registry).await?;
    assert!(
        h.gateway().sync(h.actor.user()).await.is_err(),
        "RED: guarded marker with missing request ref cannot downgrade"
    );
    assert!(h.writes().is_empty());
    Ok(())
}

#[tokio::test]
async fn outbox_endpoint_capability_loss_retains_guarded_item_without_event_downgrade() -> Result<()>
{
    let h = Harness::new().await?;
    let event = h.checked_decline().await?;
    h.protocol.capability.store(0, Ordering::SeqCst);
    assert!(
        h.reopened().sync(h.actor.user()).await.is_err(),
        "RED: endpoint support must be rechecked before guarded upload"
    );
    assert!(h.writes().is_empty());
    let events = h.events().await?;
    assert!(
        events.len() == 1 && events[0] == event,
        "unsupported endpoint must preserve the exact pending item"
    );
    h.protocol.enable_current_guarded();
    h.reopened().sync(h.actor.user()).await?;
    assert_guarded(&h.writes()[0], &event, &h.actor)?;
    Ok(())
}

#[tokio::test]
async fn outbox_preboundary_old_plain_removal_keeps_its_original_legacy_signature() -> Result<()> {
    let h = Harness::new().await?;
    legacy_append(&h).await?;
    let event = h.events().await?.pop().context("legacy event")?;
    let action = h
        .reopened()
        .sign_queued_device_event(event.clone(), &h.actor.user())
        .await
        .context("RED: real pre-upgrade queued removal must remain readable")?;
    action.verify(&h.actor.device().keys.dsa_pk)?;
    assert!(matches!(action.payload, SignedActionPayload::Event(_)));
    assert!(action.event()?.obj_id() == event.obj_id());
    Ok(())
}

#[tokio::test]
async fn outbox_explicit_legacy_owner_after_boundary_stays_legacy_without_forging_guard()
-> Result<()> {
    let h = Harness::new().await?;
    let event = h.install(RemovalEnvelopeMode::Legacy).await?;
    let action = h
        .gateway()
        .sign_queued_device_event(event.clone(), &h.actor.user())
        .await
        .context("RED: explicit compatibility producer owns legacy envelope")?;
    action.verify(&h.actor.device().keys.dsa_pk)?;
    assert!(matches!(action.payload, SignedActionPayload::Event(_)));
    assert!(action.event()?.obj_id() == event.obj_id());
    Ok(())
}

#[tokio::test]
async fn outbox_marker_only_crash_never_uploads_and_ordinary_append_safely_reuses_nonce()
-> Result<()> {
    let h = Harness::new().await?;
    let orphan = h.event().await?;
    h.registry(&orphan, RemovalEnvelopeMode::Guarded).await?;
    h.reopened().sync(h.actor.user()).await?;
    assert!(h.writes().is_empty(), "marker alone is not a user command");
    PersistentDeviceLog::from(h.p_obj.clone())
        .save_create_vault_request(&h.actor.user())
        .await?;
    let events = h.events().await?;
    assert!(events.len() == 1 && events[0].obj_id() == orphan.obj_id());
    let registry = PersistentRemovalEnvelope {
        p_obj: h.p_obj.clone(),
    }
    .load(&h.actor.user_id())
    .await?;
    assert!(
        registry.is_none_or(|record| record
            .0
            .value
            .entries
            .iter()
            .all(|entry| entry.event_id != orphan.obj_id())),
        "RED: orphan ownership must be removed/verified before nonce reuse for a different command"
    );
    Ok(())
}

#[tokio::test]
async fn outbox_database_reset_drops_guarded_ownership_and_no_command_revives() -> Result<()> {
    let h = Harness::new().await?;
    h.install(RemovalEnvelopeMode::Guarded).await?;
    h.p_obj.repo.db_clean_up().await;
    assert!(
        PersistentRemovalEnvelope {
            p_obj: h.p_obj.clone()
        }
        .load(&h.actor.user_id())
        .await?
        .is_none()
    );
    assert!(h.events().await?.is_empty());
    let _ = h.reopened().sync(h.actor.user()).await;
    assert!(h.writes().is_empty());
    Ok(())
}

#[tokio::test]
async fn outbox_failed_orphan_cleanup_forbids_reusing_reserved_nonce() -> Result<()> {
    let h = Harness::new().await?;
    let orphan = h.event().await?;
    h.registry(&orphan, RemovalEnvelopeMode::Guarded).await?;
    h.p_obj.repo.arm(Fault::Delete);
    let result = PersistentDeviceLog::from(h.p_obj.clone())
        .save_create_vault_request(&h.actor.user())
        .await;
    assert!(
        result.is_err(),
        "RED: failed orphan cleanup must forbid reuse for a different command"
    );
    assert!(h.events().await?.is_empty());
    assert!(
        h.p_obj.repo.hits.load(Ordering::SeqCst) > 0,
        "fallible deletion/update of orphan ownership must actually run"
    );
    Ok(())
}
