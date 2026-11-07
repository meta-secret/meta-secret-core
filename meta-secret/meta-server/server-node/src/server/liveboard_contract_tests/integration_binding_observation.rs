//! Atomic unsigned metadata observation is not signed authority or a new action.
use super::integration_binding_fixture::{FaultRepo, Seed};
use super::*;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        api::{ServerTailRequest, ServerTailResponse},
        common::model::liveboard_producer::*,
        db::repo::generic_db::FindOneQuery,
    },
};

fn reference(source: &DeviceLogObject, request_id: &str) -> Result<ProducerReference> {
    let canonical = String::from_utf8(canonical_json(source)?)?;
    Ok(ProducerReference {
        version: 1,
        kind: ProducerKind::DeleteSecret,
        request_id: Some(request_id.into()),
        request_stage: ProducerSourceRef {
            event_id: source.obj_id(),
            body_commitment: sha256_hex(&format!(
                "MetaSecret.LiveboardProducer.Source.v1\n{canonical}"
            )),
        },
        decision_stage: None,
    })
}
async fn observe<R: KvLogEventRepo>(
    server: &ServerApp<R>,
    actor: &UserCreds,
    input: ProducerReference,
) -> Result<ServerTailResponse> {
    server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
            ServerTailRequest {
                sender: actor.user(),
                producer_reference: Some(input),
            },
        ))))
        .await?
        .to_server_tail()
}
fn assert_safe(observation: &ProducerCanonicalObservation) -> Result<()> {
    let wire = serde_json::to_value(observation)?;
    let object = wire.as_object().context("typed observation")?;
    let allow = [
        "version",
        "kind",
        "secretId",
        "targetDeviceId",
        "requestId",
        "requestStage",
        "decisionStage",
        "canonicalState",
    ];
    assert!(object.keys().all(|key| allow.contains(&key.as_str())));
    let json = serde_json::to_string(observation)?;
    for forbidden in [
        "signature",
        "shareTransfers",
        "cipher",
        "masterKey",
        "PRIVATE-",
        "baseVaultArtifactId",
        "targetVaultArtifactId",
        "automaticDeleteApplication",
        "signedActionHash",
        "readiness",
        "materialStage",
    ] {
        assert!(
            !json.contains(forbidden),
            "observation must not publish private phase/proof/body or local success"
        );
    }
    Ok(())
}

#[tokio::test]
async fn integration_binding_observation_equal_next_tail_means_free_not_acknowledged() -> Result<()>
{
    let h = Seed::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    let server = h.server().await?;
    let (source, _, id) = h.deletion().await?;
    let query = reference(&source, &id)?;
    let before = h.repo.get_db().await;
    let response = observe(&server, &h.actor, query.clone()).await?;
    assert_eq!(
        response.device_log_tail,
        Some(source.obj_id()),
        "NEXT tail equals N: N is still free"
    );
    let observation = response
        .producer_observation
        .context("RED: typed same-boundary source observation absent")?;
    assert_eq!(observation.request_stage.source, query.request_stage);
    assert_eq!(
        observation.request_stage.observation,
        ProducerSourceObservation::Absent
    );
    assert_eq!(observation.canonical_state, ProducerCanonicalState::Unknown);
    assert_safe(&observation)?;
    assert!(
        h.repo.get_db().await == before,
        "unsigned clean observation must create no producer/source/decision"
    );
    Ok(())
}

#[tokio::test]
async fn integration_binding_observation_admitted_source_wins_over_raw_absence_then_raw_ack()
-> Result<()> {
    let repo = Arc::new(FaultRepo::new(Arc::new(InMemKvLogEventRepo::default())));
    let h = Seed::new(repo.clone(), 3).await?;
    let server = h.server().await?;
    let (source, signed, id) = h.deletion().await?;
    let query = reference(&source, &id)?;
    repo.before_raw();
    assert!(send(&server, signed.clone()).await.is_err());
    assert!(repo.inner.find_one(source.obj_id()).await?.is_none());
    let response = observe(&server, &h.actor, query.clone()).await?;
    assert_eq!(response.device_log_tail, Some(source.obj_id()));
    let observation = response
        .producer_observation
        .context("RED: real admitted source mapping not observed")?;
    assert_eq!(observation.request_id.as_deref(), Some(id.as_str()));
    assert_eq!(observation.request_stage.source, query.request_stage);
    assert_eq!(
        observation.request_stage.observation,
        ProducerSourceObservation::Admitted
    );
    assert_eq!(observation.canonical_state, ProducerCanonicalState::Pending);
    assert_safe(&observation)?;
    send(&server, signed).await?;
    let response = observe(&server, &h.actor, query.clone()).await?;
    assert_eq!(response.device_log_tail, Some(source.obj_id().next()));
    let observed = response
        .producer_observation
        .context("same source after raw ACK")?;
    assert_eq!(observed.request_id, Some(id));
    assert_eq!(
        observed.request_stage.observation,
        ProducerSourceObservation::RawAcknowledged
    );
    assert_eq!(observed.canonical_state, ProducerCanonicalState::Pending);
    assert_safe(&observed)
}

#[tokio::test]
async fn integration_binding_observation_uses_one_ordered_snapshot_during_concurrent_admission()
-> Result<()> {
    let repo = Arc::new(FaultRepo::new(Arc::new(InMemKvLogEventRepo::default())));
    let h = Seed::new(repo.clone(), 3).await?;
    let server = h.server().await?;
    let (source, signed, id) = h.deletion().await?;
    let query = reference(&source, &id)?;
    repo.query_barrier.arm(source.obj_id());
    let scenario = async {
        let observing = observe(&server, &h.actor, query.clone());
        let racing = async {
            repo.query_barrier.wait().await;
            let mut write = Box::pin(send(&server, signed));
            assert!(
                std::future::poll_fn(|cx| std::task::Poll::Ready(write.as_mut().poll(cx)))
                    .await
                    .is_pending(),
                "real server owner must hold concurrent admission until observation finishes"
            );
            assert!(repo.inner.find_one(source.obj_id()).await?.is_none());
            repo.query_barrier.release();
            write.await
        };
        let (before, written) = tokio::join!(observing, racing);
        written?;
        let before = before?;
        assert_eq!(before.device_log_tail, Some(source.obj_id()));
        let observed = before
            .producer_observation
            .context("RED: observation missing from held actual server snapshot")?;
        assert_eq!(
            observed.request_stage.observation,
            ProducerSourceObservation::Absent
        );
        assert_eq!(observed.canonical_state, ProducerCanonicalState::Unknown);
        let after = observe(&server, &h.actor, query).await?;
        assert_eq!(after.device_log_tail, Some(source.obj_id().next()));
        let observed = after
            .producer_observation
            .context("post-admission observation")?;
        assert_eq!(
            observed.request_stage.observation,
            ProducerSourceObservation::RawAcknowledged
        );
        assert_eq!(observed.canonical_state, ProducerCanonicalState::Pending);
        Ok(())
    };
    tokio::select! {result=scenario=>result,_=tokio::time::sleep(std::time::Duration::from_secs(10))=>anyhow::bail!("atomic observation watchdog; no ordering inferred from time")}
}

#[tokio::test]
async fn integration_binding_observation_nested_unknown_source_is_rejected_at_actual_wire_decode()
-> Result<()> {
    let h = Seed::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    let (source, _, id) = h.deletion().await?;
    let wire = serde_json::to_value(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
        ServerTailRequest {
            sender: h.actor.user(),
            producer_reference: Some(reference(&source, &id)?),
        },
    ))))?;
    for path in [
        "/read/serverTail/producerReference/requestStage/eventId",
        "/read/serverTail/producerReference/requestStage/eventId/fqdn",
        "/read/serverTail/producerReference/requestStage/eventId/id",
    ] {
        let mut changed = wire.clone();
        changed
            .pointer_mut(path)
            .context("actual nested protocol reference")?
            .as_object_mut()
            .context("nested object")?
            .insert("unexpected".into(), json!(true));
        assert!(
            serde_json::from_value::<SyncRequest>(changed).is_err(),
            "RED: actual wire decoder silently ignores source nested fields at {path}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn integration_binding_observation_legacy_tail_defaults_and_omissions_control() -> Result<()>
{
    let h = Seed::new(Arc::new(InMemKvLogEventRepo::default()), 1).await?;
    let server = h.server().await?;
    let old: SyncRequest =
        serde_json::from_value(json!({"read":{"serverTail":{"sender":h.actor.user()}}}))?;
    let response = server.handle_client_request(old).await?.to_server_tail()?;
    assert!(response.producer_observation.is_none());
    assert!(
        serde_json::to_value(&response)?
            .get("producerObservation")
            .is_none()
    );
    let old: meta_secret_core::node::api::LiveboardCapabilities =
        serde_json::from_value(json!({"projectionVersion":1,"guardedRemovalVersion":1}))?;
    assert_eq!(old.producer_correlation_version, 0);
    Ok(())
}
