//! Local metadata corruption after a real healthy checked preparation/body.
use super::{stage::private_crypto, support::H, *};
use meta_secret_core::node::{
    common::model::liveboard::LiveboardOperationPhase,
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::local_removal_envelope::RemovalEnvelopeMode,
        objects::local_removal_envelope::PersistentRemovalEnvelope,
    },
};

#[derive(Clone, Copy, Debug)]
enum Corruption {
    Legacy,
    WrongUuid,
    MissingEntry,
}

#[tokio::test]
async fn actual_queued_guarded_retry_rejects_legacy_wrong_uuid_or_missing_registry_entry()
-> Result<()> {
    // Collect every actual variant before propagating its assertion outcome.
    let mut results = vec![];
    for variant in [
        Corruption::Legacy,
        Corruption::WrongUuid,
        Corruption::MissingEntry,
    ] {
        results.push(run(variant).await);
    }
    for result in results {
        result?;
    }
    Ok(())
}

async fn run(variant: Corruption) -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let queued = h.decide().await?;
    ensure!(
        queued.phase == LiveboardOperationPhase::Queued && queued.request_id == h.state.uuid,
        "actual healthy checked body required before corruption"
    );
    let body =
        h.p.repo
            .inner
            .find_one(queued.event_id.clone())
            .await?
            .context("actual persisted checked body")?;
    let frozen = h.witness().await?;
    private_crypto(&h.state, &frozen)?;
    let registry_store = PersistentRemovalEnvelope { p_obj: h.p.clone() };
    let mut registry = registry_store
        .load(&h.state.b.user_id())
        .await?
        .context("actual existing Guarded registry")?;
    ensure!(
        registry.0.value.first_managed_removal_nonce > 0,
        "positive managed boundary must remain"
    );
    let entry = registry
        .0
        .value
        .entries
        .iter_mut()
        .find(|entry| entry.event_id == queued.event_id)
        .context("healthy actual body envelope entry")?;
    ensure!(
        entry.mode == RemovalEnvelopeMode::Guarded
            && entry.request_id.as_deref() == Some(&h.state.uuid),
        "initial producer did not own exact Guarded UUID"
    );
    match variant {
        Corruption::Legacy => {
            entry.mode = RemovalEnvelopeMode::Legacy;
            entry.request_id = None;
        }
        Corruption::WrongUuid => {
            // Syntactically valid but deliberately NOT this real canonical UUID.
            let other = "00000000-0000-4000-8000-000000000000";
            ensure!(
                is_liveboard_request_id(other) && other != h.state.uuid,
                "distinct valid corruption UUID fixture required"
            );
            entry.request_id = Some(other.into());
        }
        Corruption::MissingEntry => {
            registry
                .0
                .value
                .entries
                .retain(|entry| entry.event_id != queued.event_id);
        }
    }
    // ONLY the one existing local registry record is corrupted. No canonical mutation.
    h.p.repo.inner.replace(registry.clone()).await?;
    ensure!(
        registry_store.load(&h.state.b.user_id()).await? == Some(registry),
        "fixture must remain structurally valid to isolate exact ownership binding"
    );
    let before = h.p.repo.snapshot().await?;
    let result = h.decide().await;
    let after = h.p.repo.snapshot().await?;
    let body_unchanged = h.p.repo.inner.find_one(queued.event_id.clone()).await? == Some(body);
    let private_unchanged = h.witness().await? == frozen;
    let own_stream = ArtifactId::from(DeviceLogDescriptor::from(h.state.b.user_id())).fqdn;
    let own_bodies = after
        .values()
        .filter(|event| matches!(event, GenericKvLogEvent::DeviceLog(body) if body.obj_id().fqdn == own_stream))
        .count();
    eprintln!(
        "C1b registry {variant:?} unavailable={} db_unchanged={} body_unchanged={body_unchanged} private_unchanged={private_unchanged} own_bodies={own_bodies} writes={}",
        result.is_err(),
        after == before,
        h.wire.writes.load(Ordering::SeqCst)
    );
    let error = result
        .err()
        .context("exact queued retry accepted corrupted Guarded ownership")?;
    ensure!(
        error.to_string() == "GuardedPreparationUnavailable",
        "corrupt ownership must use existing static unavailable error"
    );
    ensure!(
        after == before && body_unchanged && private_unchanged && own_bodies == 1,
        "rejected retry mutated/reclassified/requeued/recomputed existing operation"
    );
    ensure!(
        h.wire.writes.load(Ordering::SeqCst) == 0,
        "registry corruption must not reach any transport write"
    );
    Ok(())
}
