use super::recovery_receipt_faults::{Fault, ReceiptFaultRepo, Step};
use super::recovery_receipt_fixture::ReceiptHarness;
use super::*;
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::db::repo::generic_db::FindOneQuery;
use std::io::Write;
use std::sync::Mutex;

pub(super) fn cipher_marker(packet: &SignedAction) -> Result<String> {
    match packet.event()? {
        GenericKvLogEvent::SsWorkflow(
            meta_secret_core::node::db::events::shared_secret_event::SsWorkflowObject::Recovery(
                value,
            ),
        ) => Ok(value
            .value
            .secret_message
            .cipher_text()
            .msg
            .base64_str()
            .to_owned()),
        _ => anyhow::bail!("expected encrypted approval fixture"),
    }
}
fn assert_private_absent(value: &impl Serialize, marker: &str) -> Result<()> {
    let text = serde_json::to_string(value)?;
    assert!(
        !text.contains(marker),
        "private encrypted packet leaked into a public response"
    );
    assert!(
        !text.contains("baseSsLogArtifactId") && !text.contains("\"packet\""),
        "private application row leaked into a public response"
    );
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_public_history_fallback_completion_and_generic_tails_never_export_private_packet()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(ReceiptFaultRepo::new(inner.clone()));
    let h = ReceiptHarness::new(repo.clone()).await?;
    let publisher = Arc::new(super::recovery_intake_fixture::Publisher::default());
    let server = ServerApp::new_with_invalidation_publisher(
        repo.clone(),
        h.base.master_key.clone(),
        publisher.clone(),
    )?;
    server.init().await?;
    let packet = h.packet(&h.base.b, true)?;
    let marker = cipher_marker(&packet)?;
    repo.arm(Step::Raw, Fault::Before);
    assert!(send(&server, packet).await.is_err());
    let row = inner
        .server_recovery_response_store()?
        .load(&h.claim.vault_name)
        .await?
        .context("RED: private verified reservation missing")?;
    assert_private_absent(
        &inner.get_db().await.into_values().collect::<Vec<_>>(),
        &marker,
    )?;
    repo.clear();
    // Receiver reads public SsLog history and the latest fallback, never the requester's raw packet.
    for id in [
        row.base_ss_log_artifact_id.clone(),
        row.target_ss_log_artifact_id.clone().next().next(),
    ] {
        let response = server
            .handle_client_request(h.ss_request(&h.base.c, Some(id)))
            .await?;
        assert_private_absent(&response, &marker)?;
    }
    let response = server.handle_client_request(h.completion()?).await?;
    assert_private_absent(&response, &marker)?;
    for field in 0..3 {
        let mut tail = VaultTail {
            vault: ArtifactId::from(VaultDescriptor::from(h.claim.vault_name.clone())),
            vault_log: ArtifactId::from(VaultLogDescriptor::from(h.claim.vault_name.clone())),
            vault_status: ArtifactId::from(VaultStatusDescriptor::from(h.base.c.user().user_id())),
        };
        let private = ArtifactId::from(
            meta_secret_core::node::db::descriptors::object_descriptor::ObjectFqdn {
                obj_type: "server_recovery_response_private".into(),
                obj_instance: h.claim.vault_name.0.clone(),
            },
        );
        match field {
            0 => tail.vault = private,
            1 => tail.vault_log = private,
            _ => tail.vault_status = private,
        }
        let before = inner.get_db().await;
        let private_before = inner
            .server_recovery_response_store()?
            .load(&h.claim.vault_name)
            .await?;
        let published = publisher.0.load(std::sync::atomic::Ordering::SeqCst);
        let error = server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
                VaultRequest {
                    sender: h.base.c.user(),
                    tail,
                },
            ))))
            .await
            .err()
            .context("private descriptor accepted as public tail")?;
        assert_eq!(error.to_string(), "InvalidPublicReadTail");
        assert!(inner.get_db().await == before);
        assert!(
            inner
                .server_recovery_response_store()?
                .load(&h.claim.vault_name)
                .await?
                == private_before
        );
        assert_eq!(
            publisher.0.load(std::sync::atomic::Ordering::SeqCst),
            published
        );
    }
    for event in inner.get_db().await.into_values() {
        if matches!(event, GenericKvLogEvent::SsLog(_)) {
            assert_private_absent(&event, &marker)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_forged_sender_rejected_before_valid_actual_vault_repair() -> Result<()> {
    for stream in 0..4 {
        let inner = Arc::new(InMemKvLogEventRepo::default());
        let repo = Arc::new(ReceiptFaultRepo::new(inner.clone()));
        let h = ReceiptHarness::new(repo.clone()).await?;
        let publisher = Arc::new(super::recovery_intake_fixture::Publisher::default());
        let server = ServerApp::new_with_invalidation_publisher(
            repo.clone(),
            h.base.master_key.clone(),
            publisher.clone(),
        )?;
        server.init().await?;
        let packet = h.packet(&h.base.b, true)?;
        let marker = cipher_marker(&packet)?;
        repo.arm(Step::Raw, Fault::Before);
        assert!(send(&server, packet).await.is_err());
        repo.clear();
        let mut sender = h.base.c.user();
        sender.vault_name = meta_secret_core::node::common::model::vault::vault::VaultName::from(
            "different-sender-vault",
        );
        let actual = ArtifactId::from(meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsLogDescriptor::from(h.claim.vault_name.clone()));
        let unrelated = ArtifactId::from(VaultDescriptor::from(sender.vault_name.clone()));
        let request = if stream == 3 {
            SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                meta_secret_core::node::api::SsRequest {
                    sender,
                    ss_log: actual,
                },
            )))
        } else {
            let mut tail = VaultTail {
                vault: unrelated.clone(),
                vault_log: unrelated.clone(),
                vault_status: unrelated,
            };
            match stream {
                0 => tail.vault = actual,
                1 => tail.vault_log = actual,
                _ => tail.vault_status = actual,
            }
            SyncRequest::Read(Box::new(ReadSyncRequest::Vault(VaultRequest {
                sender,
                tail,
            })))
        };
        let before = inner.get_db().await;
        let private_before = inner
            .server_recovery_response_store()?
            .load(&h.claim.vault_name)
            .await?
            .context("actual reserved private Recovery packet")?;
        assert!(private_before.packet.is_some());
        let published = publisher.0.load(std::sync::atomic::Ordering::SeqCst);
        let error = server
            .handle_client_request(request)
            .await
            .err()
            .context("forged sender tail accepted")?;
        assert_eq!(error.to_string(), "InvalidPublicReadTail");
        assert!(inner.get_db().await == before);
        assert!(
            inner
                .server_recovery_response_store()?
                .load(&h.claim.vault_name)
                .await?
                == Some(private_before)
        );
        assert_eq!(
            publisher.0.load(std::sync::atomic::Ordering::SeqCst),
            published
        );
        let request = if stream == 3 {
            h.ss_request(&h.base.c, None)
        } else {
            SyncRequest::Read(Box::new(ReadSyncRequest::Vault(VaultRequest {
                sender: h.base.c.user(),
                tail: VaultTail {
                    vault: ArtifactId::from(VaultDescriptor::from(h.claim.vault_name.clone())),
                    vault_log: ArtifactId::from(VaultLogDescriptor::from(
                        h.claim.vault_name.clone(),
                    )),
                    vault_status: ArtifactId::from(VaultStatusDescriptor::from(
                        h.base.c.user().user_id(),
                    )),
                },
            })))
        };
        let response = server.handle_client_request(request).await?;
        assert_private_absent(&response, &marker)?;
        h.assert_winner(true)
            .await
            .context("RED: valid actual Vault read bypassed private Recovery repair")?;
    }
    Ok(())
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
async fn recovery_receipt_trace_debug_and_private_errors_are_payload_free() -> Result<()> {
    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(capture.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(ReceiptFaultRepo::new(inner.clone()));
    let h = ReceiptHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    let packet = h.packet(&h.base.b, true)?;
    let marker = cipher_marker(&packet)?;
    let row = h.prepared(packet.clone(), true).await?;
    assert!(
        !format!("{row:?}").contains(&marker),
        "private Debug must redact encrypted content"
    );
    repo.arm(Step::Mark, Fault::Before);
    let error = send(&server, packet)
        .await
        .expect_err("RED: private phase fault must fail the real request");
    assert!(
        !format!("{error:?}").contains(&marker),
        "private error chains must contain static safe codes only"
    );
    let trace = String::from_utf8(capture.0.lock().unwrap().clone())?;
    assert!(
        !trace.contains(&marker),
        "TRACE instrumentation leaked the encrypted packet"
    );
    assert!(
        !trace.contains("private-receipt-synthetic-share"),
        "TRACE must not contain synthetic plaintext"
    );
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_conflicting_raw_packet_fails_without_overwrite_or_new_winner()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(ReceiptFaultRepo::new(inner.clone()));
    let h = ReceiptHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    let original = h.packet(&h.base.b, true)?;
    repo.arm(Step::Raw, Fault::Before);
    assert!(send(&server, original).await.is_err());
    let row = inner
        .server_recovery_response_store()?
        .load(&h.claim.vault_name)
        .await?
        .context("RED: reservation before raw failure")?;
    let conflicting = h.packet(&h.base.b, true)?.event()?.clone();
    inner.save(conflicting.clone()).await?;
    repo.clear();
    assert!(
        refresh(&server, &h.base.a).await.is_err(),
        "different packet at frozen raw ID must remain unavailable"
    );
    assert!(inner.find_one(conflicting.obj_id()).await? == Some(conflicting));
    assert!(
        h.log().await?.liveboard_recovery_results.is_empty(),
        "a raw conflict cannot invent a committed winner"
    );
    assert!(
        inner
            .server_recovery_response_store()?
            .load(&h.claim.vault_name)
            .await?
            == Some(row)
    );
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_conflicting_canonical_target_fails_without_replacing_snapshot()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(ReceiptFaultRepo::new(inner.clone()));
    let h = ReceiptHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    repo.arm(Step::Canonical, Fault::Before);
    assert!(send(&server, h.packet(&h.base.b, true)?).await.is_err());
    let row = inner
        .server_recovery_response_store()?
        .load(&h.claim.vault_name)
        .await?
        .context("RED: reservation before canonical failure")?;
    let mut conflict = inner
        .find_one(row.base_ss_log_artifact_id.clone())
        .await?
        .context("base snapshot")?;
    if let GenericKvLogEvent::SsLog(log) = &mut conflict {
        log.0.key.obj_id = row.target_ss_log_artifact_id.clone();
        log.0.value.claims.clear();
    }
    inner.save(conflict.clone()).await?;
    repo.clear();
    assert!(
        refresh(&server, &h.base.a).await.is_err(),
        "different snapshot at the frozen canonical ID must remain unavailable"
    );
    assert!(
        inner
            .find_one(row.target_ss_log_artifact_id.clone())
            .await?
            == Some(conflict)
    );
    assert!(
        inner
            .server_recovery_response_store()?
            .load(&h.claim.vault_name)
            .await?
            == Some(row)
    );
    Ok(())
}
