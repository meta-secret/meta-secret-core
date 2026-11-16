use super::super::{
    correlation_faults::{CorrelationRepo, Fault, Step, transfer_count},
    correlation_support as proof,
    producer_guard_fixture::GuardFixture,
};
use super::{support::*, *};
use meta_secret_core::node::common::model::liveboard_producer::*;

#[tokio::test]
async fn noop_b_actual_canonical_applied_raw_gap_never_becomes_business_raw_ack() -> Result<()> {
    // Server-only actual automatic Delete; not a claim that client retirement R/U ran.
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(CorrelationRepo::new(inner.clone()));
    let h = Seed::new(repo.clone(), 1).await?;
    let publisher = Arc::new(super::super::recovery_intake_fixture::Publisher::default());
    let server = ServerApp::new_with_invalidation_publisher(
        repo.clone(),
        h.master.clone(),
        publisher.clone(),
    )?;
    server.init().await?;
    let (source, signed, uuid) = h.deletion().await?;
    repo.arm(&source, &uuid, Step::Raw, Fault::Before);
    assert!(send(&server, signed.clone()).await.is_err());
    assert_eq!(
        repo.hits.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "actual raw-save fault must be reached exactly once after canonical outcome"
    );
    repo.clear();
    assert!(inner.find_one(source.obj_id()).await?.is_none());
    let vault = PersistentVault::from(h.p.clone())
        .get_vault(h.actor.vault_name.clone())
        .await?;
    assert!(vault.0.value.deletion_results[&uuid].accepted);
    let retained = proof::source_ref(&h.log().await?, &source, &uuid)?;
    let mut auth = signed_value(&h.actor, source.obj_id())?;
    auth["clientEntryCommitment"] = json!(proof::commitment(&source)?);
    submit(&server, &sign_value(&h.actor, auth)?).await?;
    let marker = inner
        .find_one(source.obj_id())
        .await?
        .context("actual Noop at missing raw N")?;
    assert!(matches!(marker, GenericKvLogEvent::DeviceTransportNoop(_)));
    assert_eq!(
        PersistentVault::from(h.p.clone())
            .get_vault(h.actor.vault_name.clone())
            .await?,
        vault
    );
    assert_eq!(
        proof::source_ref(&h.log().await?, &source, &uuid)?,
        retained
    );
    let observed = proof::observe(&server, h.actor.user(), proof::reference(&source, &uuid)?)
        .await.context("B actual canonical Applied + exact Noop reached; source observation rejects technical marker")?
        .producer_observation
        .context("canonical source observation after technical fill")?;
    assert_eq!(observed.canonical_state, ProducerCanonicalState::Applied);
    assert_ne!(
        observed.request_stage.observation,
        ProducerSourceObservation::RawAcknowledged
    );
    let before_rejected = inner.get_db().await;
    let published = publisher.0.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        send(&server, signed).await.is_err(),
        "late original Business must not overwrite Noop or reapply domain command"
    );
    assert!(
        inner.get_db().await == before_rejected,
        "rejected late original may not change canonical state/raw marker"
    );
    assert_eq!(
        publisher.0.load(std::sync::atomic::Ordering::SeqCst),
        published
    );
    assert_eq!(inner.find_one(source.obj_id()).await?, Some(marker));
    assert_eq!(
        proof::source_ref(&h.log().await?, &source, &uuid)?,
        retained
    );
    Ok(())
}

pub(super) fn vault_request<R: KvLogEventRepo>(h: &Seed<R>) -> VaultRequest {
    VaultRequest {
        sender: h.actor.user(),
        tail: VaultTail {
            vault_status: ArtifactId::from(VaultStatusDescriptor::from(h.actor.user_id())),
            vault_log: ArtifactId::from(VaultLogDescriptor::from(h.actor.vault_name.clone())),
            vault: ArtifactId::from(VaultDescriptor::from(h.actor.vault_name.clone())),
        },
    }
}
#[tokio::test]
async fn noop_b_indexed_admitted_remove_source_now_marker_keeps_pending_uuid_and_public_reads()
-> Result<()> {
    let h = GuardFixture::new(true).await?;
    let retained = h.retained_ref().await?;
    submit(&h.server, &signed_value(&h.seed.actor, h.source.obj_id())?).await?;
    let marker = h
        .seed
        .repo
        .inner
        .find_one(h.source.obj_id())
        .await?
        .context("actual marker")?;
    let request = vault_request(&h.seed);
    let outer = h
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            request.clone(),
        ))))
        .await;
    let gateway = crate::server::server_data_sync::ServerSyncGateway::new(
        h.seed.p.clone(),
        h.publisher.clone(),
    );
    let direct = gateway.vault_replication(request).await;
    eprintln!(
        "actual indexed Noop public read: outer_error={} direct_error={}",
        outer.is_err(),
        direct.is_err()
    );
    let response = outer.context(
        "B indexed admitted sourceN holds technical marker; legitimate outer read must progress",
    )?;
    assert!(matches!(response, DataSyncResponse::Data(_)));
    let direct = direct.context("B indexed admitted sourceN technical marker direct read")?;
    assert!(!direct.is_empty());
    assert_eq!(h.retained_ref().await?, retained);
    assert_eq!(
        h.seed.repo.inner.find_one(h.source.obj_id()).await?,
        Some(marker)
    );
    let reference = h.reference().await?;
    let observed = proof::observe(&h.server, h.seed.actor.user(), reference)
        .await?
        .producer_observation
        .context("pending source remains observable")?;
    assert_eq!(observed.canonical_state, ProducerCanonicalState::Pending);
    assert_ne!(
        observed.request_stage.observation,
        ProducerSourceObservation::RawAcknowledged
    );
    Ok(())
}
#[tokio::test]
async fn noop_b_marker_then_real_encrypted_remove_body_purge_continues_and_preserves_marker()
-> Result<()> {
    let h = GuardFixture::new(false).await?;
    let id = h.source.obj_id();
    submit(&h.server, &signed_value(&h.seed.actor, id.clone())?).await?;
    let marker = h
        .seed
        .repo
        .inner
        .find_one(id.clone())
        .await?
        .context("actual prefix marker")?;
    // Request body/transfers came from actual Core MetaOrchestrator, never fabricated ciphertext.
    let source = next_event(&h.seed.p, &h.seed.actor, h.source.0.value.clone()).await?;
    assert_eq!(source.obj_id(), id.clone().next());
    assert!(transfer_count(&serde_json::to_value(&source)?) > 0);
    let signed = sign_event(
        source.clone().to_generic(),
        h.seed.actor.device_id().clone(),
        &h.seed.actor.device_creds.key_manager()?.dsa,
    )?;
    let original = canonical_json(&signed)?;
    let outcome = send(&h.server, signed.clone()).await;
    assert!(
        h.seed.repo.inner.find_one(source.obj_id()).await?.is_some(),
        "actual later encrypted Business must have reached raw persistence before cleanup fault"
    );
    outcome.context(
        "B actual later encrypted source persisted; cleanup cannot traverse earlier Noop",
    )?;
    assert_eq!(canonical_json(&signed)?, original);
    let scrubbed = h
        .seed
        .repo
        .inner
        .find_one(source.obj_id())
        .await?
        .context("actual later Business")?
        .device_log()?;
    assert_eq!(transfer_count(&serde_json::to_value(scrubbed)?), 0);
    assert_eq!(h.seed.repo.inner.find_one(id).await?, Some(marker));
    let log = h.seed.log().await?;
    let reference = log
        .0
        .value
        .producer_source(&source.obj_id())?
        .context("actual later source binding")?;
    assert_eq!(reference.body_commitment, proof::commitment(&source)?);
    assert_eq!(reference.source_event_id, source.obj_id());
    Ok(())
}
