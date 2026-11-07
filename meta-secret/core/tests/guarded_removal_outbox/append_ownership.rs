use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::db::actions::sign_up::remove::RemoveActionUpdate;
use meta_secret_core::node::db::objects::persistent_device_log::PersistentDeviceLog;
use std::{sync::atomic::Ordering, time::Duration};

#[tokio::test]
async fn outbox_every_device_log_helper_uses_shared_append_owner() -> Result<()> {
    let h = Harness::new().await?;
    let device_log = PersistentDeviceLog::from(h.p_obj.clone());
    let user = h.actor.user();
    let lock = h.p_obj.device_log_append_guard.lock().await;
    let append = device_log.save_create_vault_request(&user);
    tokio::pin!(append);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut append)
            .await
            .is_err(),
        "RED: DeviceLog helper bypassed the shared reserved-nonce owner"
    );
    drop(lock);
    tokio::time::timeout(Duration::from_secs(1), &mut append)
        .await
        .context("append did not resume after releasing owner")??;
    assert_eq!(h.events().await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn outbox_concurrent_join_create_and_legacy_removal_cannot_claim_same_nonce() -> Result<()> {
    let h = Harness::new().await?;
    h.p_obj.repo.yield_reads.store(true, Ordering::SeqCst);
    let first = PersistentDeviceLog::from(h.p_obj.clone());
    let second = PersistentDeviceLog::from(h.p_obj.clone());
    let removal = h.removal();
    let user = h.actor.user();
    let (create, join, decline) = tokio::join!(
        first.save_create_vault_request(&user),
        second.save_join_request(&user),
        removal.update(h.request.clone(), RemoveActionUpdate::Decline, vec![])
    );
    create?;
    join?;
    decline?;
    let events = h.events().await?;
    assert_eq!(
        events.len(),
        3,
        "RED: colliding reservations silently lost a real DeviceLog command"
    );
    let mut nonces: Vec<_> = events
        .iter()
        .map(|event| event.0.key.obj_id.id.curr)
        .collect();
    nonces.sort();
    nonces.dedup();
    assert_eq!(nonces.len(), 3);
    Ok(())
}

#[tokio::test]
async fn outbox_capability_check_then_flush_completes_without_recursive_sync_append_lock()
-> Result<()> {
    let h = Harness::new().await?;
    h.protocol.enable_current_guarded();
    let gateway = h.gateway();
    let receipt = tokio::time::timeout(
        Duration::from_secs(1),
        gateway.decide_remove_member(h.input()),
    )
    .await
    .context("exact producer deadlocked through capability/sync/append locks")??;
    tokio::time::timeout(Duration::from_secs(1), gateway.sync(h.actor.user()))
        .await
        .context("flush recursively acquired sync/append ownership")??;
    assert_eq!(h.events().await?.len(), 1);
    assert!(h.events().await?[0].0.key.obj_id == receipt.event_id);
    Ok(())
}
