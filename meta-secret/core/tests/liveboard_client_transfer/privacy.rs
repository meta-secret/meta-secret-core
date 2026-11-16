use super::{queue::futures_poll, *};
use anyhow::{Context, Result};
use meta_secret_core::node::app::meta_app::client_transfer::{
    DiagnosticTransferError as E, MetaClientRequestTransfer as Transfer,
};
use std::{
    io::{self, Write},
    pin::pin,
    sync::{Arc, Mutex},
};
use tracing::{Level, instrument::WithSubscriber};

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
async fn actual_envelope_debug_full_closed_errors_and_trace_never_dump_payload_or_reply_identity()
-> Result<()> {
    let captured = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .without_time()
        .with_ansi(false)
        .with_writer(captured.clone())
        .finish();
    let scenario = async {
        let transfer = Transfer::new();
        let mut caller = pin!(transfer.send_to_service_and_get(request("SYNTHETIC-PRIVATE-NAME")));
        assert!(futures_poll(caller.as_mut()).await.is_none());
        let call = transfer
            .service_drain()?
            .next()
            .context("actual move-only envelope")?;
        let debug = format!("{call:?}");
        assert!(
            !debug.contains("SYNTHETIC-PRIVATE")
                && !debug.contains("Sender")
                && !debug.contains("0x")
        );
        tracing::trace!(?call, "opaque local request");
        for index in 0..10 {
            transfer.send_to_service(fire(index)).await;
        }
        let full = transfer.requeue_call(call).unwrap_err();
        assert_eq!(full, E::Full);
        tracing::trace!(?full, "static diagnostic");
        assert!(matches!(
            futures_poll(caller.as_mut()).await,
            Some(Err(flume::RecvError::Disconnected))
        ));
        drop(transfer.claim_run()?);
        let closed = transfer
            .service_drain()
            .err()
            .context("closed diagnostic must reject")?;
        assert_eq!(closed, E::Closed);
        tracing::trace!(?closed, "static closed diagnostic");
        let replies = Transfer::new();
        let run = replies.claim_run()?;
        let mut private_response =
            pin!(replies.send_to_service_and_get(request("private-response-test")));
        assert!(futures_poll(private_response.as_mut()).await.is_none());
        let (_, target) = run.receive_call().await?.into_parts();
        target
            .context("private response target")?
            .respond(A::Error("SYNTHETIC-PRIVATE-RESPONSE".into()));
        assert!(
            matches!(private_response.await?, A::Error(value) if value == "SYNTHETIC-PRIVATE-RESPONSE")
        );
        anyhow::Ok(())
    };
    scenario.with_subscriber(subscriber).await?;
    let text = String::from_utf8(captured.0.lock().unwrap().clone())?;
    assert!(
        text.contains("opaque local request"),
        "capture must actually exercise Debug under TRACE"
    );
    for forbidden in [
        "SYNTHETIC-PRIVATE",
        "PlainPassInfo",
        "OperationReceipt",
        "Sender",
        "Receiver",
        "0x",
    ] {
        assert!(
            !text.contains(forbidden),
            "transient transport TRACE leaked payload/identity"
        );
    }
    Ok(())
}
