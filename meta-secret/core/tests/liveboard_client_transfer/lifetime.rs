use super::{queue::futures_poll, *};
use anyhow::{Context, Result};
use meta_secret_core::node::app::meta_app::client_transfer::{
    DiagnosticTransferError as E, MetaClientRequestTransfer as Transfer,
};
use std::pin::pin;

#[tokio::test]
async fn rejected_second_run_does_not_close_first_owners_live_reply_or_queue() -> Result<()> {
    let transfer = Transfer::new();
    let run = transfer.claim_run()?;
    assert!(matches!(transfer.claim_run(), Err(E::AlreadyOwned)));
    let mut live = pin!(transfer.send_to_service_and_get(request("only-owner")));
    assert!(futures_poll(live.as_mut()).await.is_none());
    let (_, reply) = run.receive_call().await?.into_parts();
    reply
        .context("first owner target remains live")?
        .respond(receipt("owner-id"));
    assert_receipt(live.await?, "owner-id");
    Ok(())
}

#[tokio::test]
async fn claimed_owner_startup_failure_drops_all_already_queued_private_reply_senders() -> Result<()>
{
    let transfer = Transfer::new();
    let mut first = pin!(transfer.send_to_service_and_get(request("queued-a")));
    let mut second = pin!(transfer.send_to_service_and_get(request("queued-b")));
    assert!(futures_poll(first.as_mut()).await.is_none());
    assert!(futures_poll(second.as_mut()).await.is_none());
    let guard = transfer.claim_run()?;
    drop(guard); // The claimed owner exits before its first receive (startup failure).
    for future in [&mut first, &mut second] {
        assert!(
            matches!(
                futures_poll(future.as_mut()).await,
                Some(Err(flume::RecvError::Disconnected))
            ),
            "queued caller must release without waiting for a reply/resubmit"
        );
    }
    assert!(matches!(transfer.service_drain(), Err(E::Closed)));
    Ok(())
}

#[tokio::test]
async fn owner_drop_releases_active_queued_and_capacity_blocked_callers_in_close_race() -> Result<()>
{
    let transfer = Transfer::new();
    let mut callers: Vec<_> = (0..10)
        .map(|index| {
            Box::pin(transfer.send_to_service_and_get(request(&format!("accepted-{index}"))))
        })
        .collect();
    for caller in &mut callers {
        assert!(futures_poll(caller.as_mut()).await.is_none());
    }
    let batch: Vec<_> = transfer.service_drain()?.collect();
    assert_eq!(
        batch.len(),
        10,
        "all ten original callers must actually be accepted"
    );
    for call in batch {
        transfer.requeue_call(call)?;
    }
    let mut blocked = pin!(transfer.send_to_service_and_get(request("capacity-blocked")));
    assert!(futures_poll(blocked.as_mut()).await.is_none());
    let guard = transfer.claim_run()?;
    let active = guard.receive_call().await?;
    // Receiving frees a slot for the pending sender hook; close occurs before that
    // producer resumes, covering the successful-send/post-send-closed check edge.
    drop(guard);
    drop(active);
    for caller in &mut callers {
        assert!(
            matches!(
                futures_poll(caller.as_mut()).await,
                Some(Err(flume::RecvError::Disconnected))
            ),
            "close must release each original target"
        );
    }
    assert!(
        matches!(
            futures_poll(blocked.as_mut()).await,
            Some(Err(flume::RecvError::Disconnected))
        ),
        "blocked sender must observe close, never hang behind dropped receiver"
    );
    let mut late = pin!(transfer.send_to_service_and_get(request("late-after-close")));
    assert!(matches!(
        futures_poll(late.as_mut()).await,
        Some(Err(flume::RecvError::Disconnected))
    ));
    assert!(matches!(transfer.claim_run(), Err(E::Closed)));
    Ok(())
}

#[test]
fn diagnostic_error_is_static_payload_free_without_envelope_ownership() {
    for error in [E::Full, E::AlreadyOwned, E::Closed, E::Unavailable] {
        let debug = format!("{error:?}");
        let display = error.to_string();
        assert!(
            !debug.contains("SYNTHETIC-PRIVATE-SECRET")
                && !display.contains("SYNTHETIC-PRIVATE-SECRET")
        );
        assert!(!debug.contains("Sender") && !display.contains("0x"));
    }
}
