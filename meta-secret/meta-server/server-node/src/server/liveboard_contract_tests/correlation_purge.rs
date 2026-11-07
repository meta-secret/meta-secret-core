//! Purge uses genuine retained Remove transfer history, not a fabricated ciphertext body.
use super::{
    correlation_faults::{CorrelationRepo, Fault, Step, transfer_count},
    correlation_support::{self, Resume},
    recovery_inflight_fixture::Inflight,
    *,
};
use meta_secret_core::node::{
    common::model::liveboard_producer::{
        ProducerKind, ProducerReference, ProducerSourceObservation, ProducerSourceRef,
    },
    db::events::vault::vault_log_event::DeleteSecretRequest,
    db::repo::generic_db::FindOneQuery,
};
use std::sync::atomic::Ordering;

async fn scrub(kind: Fault) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(CorrelationRepo::new(inner.clone()));
    let h = Inflight::new(repo.clone()).await?;
    let old: DeviceLogObject = h
        .local
        .find_tail_event(DeviceLogDescriptor::from(h.base.b.user_id()))
        .await?
        .context("actual signed B removal source")?;
    assert!(transfer_count(&serde_json::to_value(&old)?) > 0);
    let original_commitment = correlation_support::commitment(&old)?;
    let vaults = PersistentVault::from(Arc::new(PersistentObject::new(repo.clone())));
    let original_ref = correlation_support::source_ref(
        &vaults
            .get_vault_log_artifact(h.base.a.vault_name.clone())
            .await?,
        &old,
        &h.removal_id,
    );
    let request = DeleteSecretRequest::new(member(&h.base.a), h.pass.clone(), &h.admission.0.value);
    let id = request.request_id.clone();
    let p = Arc::new(PersistentObject::new(repo.clone()));
    let source = next_event(
        &p,
        &h.base.a,
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request)),
    )
    .await?;
    let signed = sign_event(
        source.clone().to_generic(),
        h.base.a.device_id().clone(),
        &h.base.a.device_creds.key_manager()?.dsa,
    )?;
    repo.arm(&source, &id, Step::Scrub, kind);
    let _result = send(&h.server, signed.clone()).await;
    assert!(
        repo.hits.load(Ordering::SeqCst) > 0,
        "actual retained transfer replacement/readback boundary must be reached"
    );
    let preserved = original_ref?;
    repo.clear();
    let state = Resume {
        actor: h.base.a.clone(),
        master: h.base.master_key.clone(),
        pass: h.pass.clone(),
        source,
        signed,
        id,
        workflows: vec![],
    };
    correlation_support::finish(inner.clone(), state).await?;
    let old_after = inner
        .find_one(old.obj_id())
        .await?
        .context("same original source key remains after scrub")?
        .device_log()?;
    assert_eq!(transfer_count(&serde_json::to_value(&old_after)?), 0);
    assert_ne!(
        correlation_support::commitment(&old_after)?,
        original_commitment,
        "fixture must really scrub the original signed source bytes"
    );
    let log = vaults
        .get_vault_log_artifact(h.base.a.vault_name.clone())
        .await?;
    assert_eq!(
        correlation_support::source_ref(&log, &old, &h.removal_id)?,
        preserved,
        "purge cannot rebind the original proof to scrubbed bytes"
    );
    let server = ServerApp::new(inner.clone(), h.base.master_key.clone())?;
    let input = ProducerReference {
        version: 1,
        kind: ProducerKind::RemoveDevice,
        request_id: Some(h.removal_id.clone()),
        request_stage: ProducerSourceRef {
            event_id: old.obj_id(),
            body_commitment: original_commitment,
        },
        decision_stage: None,
    };
    let observed = correlation_support::observe(&server, h.base.b.user(), input)
        .await?
        .producer_observation
        .context("original admitted ref remains observable after source redaction")?;
    assert_eq!(observed.request_id.as_deref(), Some(h.removal_id.as_str()));
    assert!(matches!(
        observed.request_stage.observation,
        ProducerSourceObservation::Redacted | ProducerSourceObservation::RawAcknowledged
    ));
    let before = inner.get_db().await;
    let _ = correlation_support::observe(
        &server,
        h.base.b.user(),
        ProducerReference {
            version: 1,
            kind: ProducerKind::RemoveDevice,
            request_id: Some(h.removal_id),
            request_stage: observed.request_stage.source,
            decision_stage: None,
        },
    )
    .await?;
    assert!(
        inner.get_db().await == before,
        "repeated observation must not restore deleted ciphertext or reapply removal"
    );
    Ok(())
}
#[tokio::test]
async fn correlation_auto_transfer_scrub_before_keeps_original_provenance() -> Result<()> {
    scrub(Fault::Before).await
}
#[tokio::test]
async fn correlation_auto_transfer_scrub_silent_drop_is_detected() -> Result<()> {
    scrub(Fault::Drop).await
}
#[tokio::test]
async fn correlation_auto_transfer_scrub_readback_fault_reopens_same_phase() -> Result<()> {
    scrub(Fault::Readback).await
}
