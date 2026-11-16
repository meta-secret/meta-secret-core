use super::{
    correlation_support as proof,
    integration_binding_fixture::{FaultRepo, Seed},
    *,
};
use std::{
    io::Write,
    sync::{Mutex, atomic::Ordering},
};

fn assert_public(value: &impl Serialize, digest: &str) -> Result<()> {
    let json = serde_json::to_string(value)?;
    for private in [
        "producerSourceRefs",
        "automaticDeleteApplication",
        "verifiedEnvelopeDigest",
        "completionMode",
        "baseVaultArtifactId",
        "targetVaultArtifactId",
        digest,
    ] {
        assert!(
            !json.contains(private),
            "private producer proof/phase leaked through a real public read"
        );
    }
    Ok(())
}
async fn generic(slot: usize) -> Result<()> {
    let h = Seed::new(Arc::new(InMemKvLogEventRepo::default()), 2).await?;
    let publisher = Arc::new(super::recovery_intake_fixture::Publisher::default());
    let server = ServerApp::new_with_invalidation_publisher(
        h.repo.clone(),
        h.master.clone(),
        publisher.clone(),
    )?;
    server.init().await?;
    let (source, signed, id) = h.deletion().await?;
    send(&server, signed.clone()).await?;
    let log = h.log().await?;
    let stored = proof::source_ref(&log, &source, &id)?;
    proof::assert_delete_ref(&stored, &source, &signed, &id)?;
    let history =
        h.p.get_object_events_from_beginning(VaultLogDescriptor::from(h.actor.vault_name.clone()))
            .await?;
    assert!(
        history.iter().any(
            |record| serde_json::to_value(&record.0.value).is_ok_and(|wire| wire
                .get("automaticDeleteApplication")
                .is_some_and(|v| !v.is_null()))
        ),
        "privacy fixture must include actual persisted automatic phase history, not empty fields"
    );
    let digest = stored["verifiedEnvelopeDigest"]
        .as_str()
        .context("real internal digest")?;
    let mut tail = VaultTail {
        vault_status: h
            .p
            .find_free_id_by_obj_desc(VaultStatusDescriptor::from(h.actor.user_id()))
            .await?,
        vault_log: h
            .p
            .find_free_id_by_obj_desc(VaultLogDescriptor::from(h.actor.vault_name.clone()))
            .await?,
        vault: h
            .p
            .find_free_id_by_obj_desc(VaultDescriptor::from(h.actor.vault_name.clone()))
            .await?,
    };
    match slot {
        0 => tail.vault_status = ArtifactId::from(VaultStatusDescriptor::from(h.actor.user_id())),
        1 => {
            tail.vault_log = ArtifactId::from(VaultLogDescriptor::from(h.actor.vault_name.clone()))
        }
        _ => tail.vault = ArtifactId::from(VaultDescriptor::from(h.actor.vault_name.clone())),
    }
    let request = VaultRequest {
        sender: h.actor.user(),
        tail,
    };
    let gateway =
        super::super::server_data_sync::ServerSyncGateway::new(h.p.clone(), publisher.clone());
    if slot != 1 {
        // The old cross-descriptor input is now a separate pre-traversal rejection phase.
        let mut malformed = request.clone();
        let wrong = ArtifactId::from(VaultLogDescriptor::from(h.actor.vault_name.clone()));
        if slot == 0 {
            malformed.tail.vault_status = wrong;
        } else {
            malformed.tail.vault = wrong;
        }
        let before = h.repo.get_db().await;
        let published = publisher.0.load(Ordering::SeqCst);
        let error = server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
                malformed.clone(),
            ))))
            .await
            .err()
            .context("malformed outer tail accepted")?;
        assert_eq!(error.to_string(), "InvalidPublicReadTail");
        assert!(h.repo.get_db().await == before);
        assert_eq!(publisher.0.load(Ordering::SeqCst), published);
        let error = gateway
            .vault_replication(malformed)
            .await
            .err()
            .context("malformed direct tail accepted")?;
        assert_eq!(error.to_string(), "InvalidPublicReadTail");
        assert!(h.repo.get_db().await == before);
        assert_eq!(publisher.0.load(Ordering::SeqCst), published);
    }
    let response = server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            request.clone(),
        ))))
        .await?;
    assert!(
        matches!(response, DataSyncResponse::Data(_)),
        "actual generic public path must execute, not reject/no-op"
    );
    assert_public(&response, digest)?;
    let direct = gateway.vault_replication(request).await?;
    assert!(
        !direct.is_empty(),
        "actual direct public path must return selected canonical data"
    );
    assert_public(&direct, digest)?;
    assert_eq!(
        proof::source_ref(&h.log().await?, &source, &id)?,
        stored,
        "public clone redaction must not erase durable private proof"
    );
    Ok(())
}
#[tokio::test]
async fn correlation_private_proof_absent_from_generic_vault_status_tail() -> Result<()> {
    generic(0).await
}
#[tokio::test]
async fn correlation_private_proof_absent_from_generic_vault_log_tail() -> Result<()> {
    generic(1).await
}
#[tokio::test]
async fn correlation_private_proof_absent_from_generic_vault_tail() -> Result<()> {
    generic(2).await
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
#[tokio::test]
async fn correlation_atomic_tail_json_and_nonempty_trace_do_not_export_private_proof() -> Result<()>
{
    let repo = Arc::new(FaultRepo::new(Arc::new(InMemKvLogEventRepo::default())));
    let h = Seed::new(repo.clone(), 3).await?;
    let server = h.server().await?;
    let (source, signed, id) = h.deletion().await?;
    repo.before_raw();
    assert!(send(&server, signed.clone()).await.is_err());
    let log = h.log().await?;
    let stored = proof::source_ref(&log, &source, &id)?;
    let digest = stored["verifiedEnvelopeDigest"]
        .as_str()
        .context("real internal digest")?;
    assert!(
        !format!("{log:?}").contains(digest),
        "private record Debug must be redacted"
    );
    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
        .with_writer(capture.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let response = proof::observe(&server, h.actor.user(), proof::reference(&source, &id)?).await?;
    assert_public(&response, digest)?;
    let observed = response
        .producer_observation
        .context("actual producer projection")?;
    assert_public(&observed, digest)?;
    let trace = String::from_utf8(capture.0.lock().unwrap().clone())?;
    assert!(
        !trace.trim().is_empty(),
        "capture must contain actual server instrumentation"
    );
    let signature = signed.signature.base64_str();
    for private in [
        digest,
        signature.as_str(),
        "PRIVATE-CORRELATION-BACKEND",
        "SYNTHETIC-PRIVATE-DELETE-SECRET",
    ] {
        assert!(
            !trace.contains(private),
            "actual read trace leaked private body/proof/error"
        );
    }
    Ok(())
}
