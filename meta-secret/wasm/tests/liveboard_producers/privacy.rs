use super::{fixture::*, repo::Fault, seed_fixture::VALUE};
use anyhow::{Context, Result};
use meta_secret_core::node::db::{
    events::vault::vault_log_event::*, in_mem_db::InMemKvLogEventRepo,
};
use std::{
    io::Write,
    sync::{Arc, Mutex},
};

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
async fn real_producer_nonempty_trace_debug_and_json_hide_transfers_plaintext_and_backend_errors()
-> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(capture.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    tracing::info!("producer trace positive control");
    let receipt = drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        h.p.repo.arm(Fault::RequestLostReply);
        h.remove(false)
            .await
            .context("RED: real producer must return actual source under lost persistence reply")
    })
    .await?;
    let sources = h.p.repo.sources();
    let source = first_request(&sources)?;
    assert_source(&receipt, source)?;
    let request = match &source.0.value {
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r)) => r,
        _ => anyhow::bail!("actual Remove with real encrypted transfers"),
    };
    assert!(
        !request.share_transfers.is_empty(),
        "nonempty real encrypted fixture required"
    );
    let trace = String::from_utf8(capture.0.lock().unwrap().clone())?;
    assert!(
        trace.contains("producer trace positive control"),
        "empty TRACE cannot pass privacy test"
    );
    let json = serde_json::to_string(&receipt)?;
    let debug = format!("{receipt:?}");
    for text in [&trace, &json, &debug] {
        assert!(!text.contains(VALUE), "synthetic plaintext leaked");
        assert!(
            !text.contains("PRIVATE-PRODUCER-PERSISTENCE-ERROR"),
            "private backend error leaked"
        );
        for transfer in &request.share_transfers {
            assert!(
                !text.contains(&transfer.encrypted_share),
                "actual Encrypted Key Share leaked"
            );
        }
    }
    assert!(!debug.contains(&receipt.request_stage.source.body_commitment));
    Ok(())
}
