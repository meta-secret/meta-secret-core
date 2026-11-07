use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::db::events::{
    generic_log_event::ObjIdExtractor, local_removal_envelope::RemovalEnvelopeMode,
};
use meta_secret_core::node::db::objects::local_removal_envelope::PersistentRemovalEnvelope;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn outbox_unsupported_capability_is_probed_before_any_local_mutation() -> Result<()> {
    capability_denied(0).await
}
#[tokio::test]
async fn outbox_capability_outage_is_not_support_and_leaves_no_preparation_or_command() -> Result<()>
{
    capability_denied(2).await
}
async fn capability_denied(mode: u8) -> Result<()> {
    let mut h = Harness::new().await?;
    // Seed real 2-of-3 encrypted local/outbound shares so the no-write assertion
    // covers preparation/resharing material, not merely an empty Vault.
    use meta_secret_core::node::common::model::meta_pass::{
        MetaPasswordId, PlainPassInfo, SecurePassInfo,
    };
    use meta_secret_core::node::db::objects::persistent_vault::PersistentVault;
    use meta_secret_core::node::db::repo::generic_db::SaveCommand;
    use meta_secret_core::secret::MetaDistributor;
    let pass_id = MetaPasswordId::build_from_str("capability-before-preparation");
    h.member.vault = h.member.vault.clone().add_secret(pass_id.clone());
    let mut vault = PersistentVault::from(h.p_obj.clone())
        .get_vault(h.actor.vault_name.clone())
        .await?;
    vault.0.value = h.member.vault.clone();
    h.p_obj.repo.replace(vault).await?;
    MetaDistributor {
        p_obj: h.p_obj.clone(),
        user_creds: std::sync::Arc::new(h.actor.clone()),
        vault_member: h.member.clone(),
    }
    .distribute(
        h.member.clone(),
        SecurePassInfo::from(PlainPassInfo {
            pass_id,
            pass: "test-only-in-memory-secret".into(),
        }),
    )
    .await?;
    h.protocol.capability.store(mode, Ordering::SeqCst);
    let before = h.p_obj.repo.snapshot().await;
    let mut input = h.input();
    input.accepted = true;
    let result = h.gateway().decide_remove_member(input).await;
    assert!(
        result.is_err(),
        "unsupported/unavailable capability must reject exact decisions"
    );
    assert!(
        h.p_obj.repo.snapshot().await == before,
        "capability failure must precede every local preparation/marker/body write"
    );
    assert!(h.writes().is_empty());
    assert_eq!(
        h.protocol.probes.load(Ordering::SeqCst),
        1,
        "RED: the actual endpoint contract must be probed, not treated as generically unavailable"
    );
    Ok(())
}

#[tokio::test]
async fn outbox_supported_exact_decision_persists_marker_before_actual_reserved_body() -> Result<()>
{
    let h = Harness::new().await?;
    h.protocol.enable_current_guarded();
    let receipt = h
        .gateway()
        .decide_remove_member(h.input())
        .await
        .context("RED: supported exact producer must queue a guarded decision")?;
    let events = h.events().await?;
    assert_eq!(events.len(), 1);
    assert!(
        events[0].obj_id() == receipt.event_id && receipt.request_id == REQUEST_ID,
        "receipt must identify the actual queued event/occurrence"
    );
    let registry = PersistentRemovalEnvelope {
        p_obj: h.p_obj.clone(),
    }
    .load(&h.actor.user_id())
    .await?
    .context("durable marker missing")?;
    let entries = &registry.0.value.entries;
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0].event_id == receipt.event_id
            && entries[0].mode == RemovalEnvelopeMode::Guarded
            && entries[0].request_id.as_deref() == Some(REQUEST_ID)
    );
    let order = h.p_obj.repo.order.lock().unwrap().clone();
    let marker = order
        .iter()
        .position(|kind| *kind == "marker")
        .context("marker save was not observed")?;
    let body = order
        .iter()
        .position(|kind| *kind == "body")
        .context("body save was not observed")?;
    assert!(
        marker < body,
        "marker ownership must be durably verified before DeviceLog append"
    );
    assert!(
        order[marker + 1..body].contains(&"marker-readback"),
        "marker must be read back before the event is appended"
    );
    Ok(())
}

#[tokio::test]
async fn outbox_marker_save_failure_never_appends_plain_removal() -> Result<()> {
    marker_fault(Fault::MarkerSave).await
}
#[tokio::test]
async fn outbox_marker_silent_drop_never_appends_plain_removal() -> Result<()> {
    marker_fault(Fault::MarkerDrop).await
}
#[tokio::test]
async fn outbox_marker_readback_failure_never_appends_plain_removal() -> Result<()> {
    marker_fault(Fault::MarkerReadback).await
}
async fn marker_fault(fault: Fault) -> Result<()> {
    let h = Harness::new().await?;
    h.protocol.enable_current_guarded();
    h.p_obj.repo.arm(fault);
    assert!(h.gateway().decide_remove_member(h.input()).await.is_err());
    assert!(
        h.events().await?.is_empty(),
        "failed marker persistence must leave no removal DeviceLog body"
    );
    assert!(
        h.writes().is_empty(),
        "failed ownership cannot sign/send a legacy fallback"
    );
    assert!(
        h.p_obj.repo.hits.load(Ordering::SeqCst) > 0,
        "RED: the real exact producer did not reach the injected marker save/readback fault"
    );
    Ok(())
}

#[tokio::test]
async fn outbox_body_saved_but_response_lost_resolves_same_id_and_remains_guarded_after_reopen()
-> Result<()> {
    let h = Harness::new().await?;
    h.protocol.enable_current_guarded();
    h.p_obj.repo.arm(Fault::BodyResponseLost);
    let receipt = h
        .gateway()
        .decide_remove_member(h.input())
        .await
        .context("RED: uncertain body save must be resolved by exact readback")?;
    h.p_obj.repo.arm(Fault::None);
    let events = h.events().await?;
    assert_eq!(
        events.len(),
        1,
        "a lost save response cannot allocate another decision nonce"
    );
    assert!(events[0].obj_id() == receipt.event_id);
    assert!(h.p_obj.repo.hits.load(Ordering::SeqCst) > 0);
    let action = h
        .reopened()
        .sign_queued_device_event(events[0].clone(), &h.actor.user())
        .await?;
    assert_guarded(&action, &events[0], &h.actor)?;
    Ok(())
}

#[tokio::test]
async fn outbox_stale_exact_reference_has_no_marker_or_command_effect() -> Result<()> {
    let h = Harness::new().await?;
    let before = h.p_obj.repo.snapshot().await;
    let mut input = h.input();
    input.request_id = "e1f621b1-ce84-40fe-a812-44abc2d79808".into();
    assert!(h.gateway().decide_remove_member(input).await.is_err());
    assert!(h.p_obj.repo.snapshot().await == before);
    assert!(h.events().await?.is_empty());
    Ok(())
}
