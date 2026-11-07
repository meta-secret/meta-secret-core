use super::{queue::futures_poll, *};
use anyhow::{Context, Result};
use meta_secret_core::node::app::meta_app::client_transfer::{
    DiagnosticTransferError as E, MetaClientRequestTransfer as Transfer,
};
use std::pin::pin;

#[tokio::test]
async fn quiescent_drain_requeue_preserves_each_original_target_once_in_batch_order() -> Result<()>
{
    let transfer = Transfer::new();
    let mut first = pin!(transfer.send_to_service_and_get(request("first")));
    let mut second = pin!(transfer.send_to_service_and_get(request("second")));
    assert!(futures_poll(first.as_mut()).await.is_none());
    assert!(futures_poll(second.as_mut()).await.is_none());
    let batch: Vec<_> = transfer.service_drain()?.collect();
    assert_eq!(batch.len(), 2);
    for call in batch {
        transfer.requeue_call(call)?;
    }
    let run = transfer.claim_run()?;
    for expected in ["first", "second"] {
        let (request, target) = run.receive_call().await?.into_parts();
        assert!(matches!(request, Q::ClusterDistributionChecked(p) if p.pass_id.name == expected));
        target
            .context("original target survives inspection")?
            .respond(receipt(expected));
    }
    assert_receipt(first.await?, "first");
    assert_receipt(second.await?, "second");
    assert!(futures_poll(pin!(run.receive_call())).await.is_none());
    Ok(())
}

#[tokio::test]
async fn interleaved_producer_precedes_requeued_original_at_its_new_admission_position()
-> Result<()> {
    let transfer = Transfer::new();
    let mut old = pin!(transfer.send_to_service_and_get(request("held-original")));
    assert!(futures_poll(old.as_mut()).await.is_none());
    let held = transfer
        .service_drain()?
        .next()
        .context("original envelope")?;
    let mut new = pin!(transfer.send_to_service_and_get(request("new-admission")));
    assert!(futures_poll(new.as_mut()).await.is_none());
    transfer.requeue_call(held)?;
    let run = transfer.claim_run()?;
    for expected in ["new-admission", "held-original"] {
        let (request, target) = run.receive_call().await?.into_parts();
        assert!(matches!(request, Q::ClusterDistributionChecked(p) if p.pass_id.name == expected));
        target
            .context("original target")?
            .respond(receipt(expected));
    }
    assert_receipt(new.await?, "new-admission");
    assert_receipt(old.await?, "held-original");
    Ok(())
}

#[tokio::test]
async fn full_requeue_drops_same_held_sender_without_touching_ten_other_queued_calls() -> Result<()>
{
    let transfer = Transfer::new();
    let mut original = pin!(transfer.send_to_service_and_get(request("held-original")));
    assert!(futures_poll(original.as_mut()).await.is_none());
    let held = transfer
        .service_drain()?
        .next()
        .context("original held envelope")?;
    for index in 0..10 {
        transfer.send_to_service(fire(index)).await;
    }
    assert_eq!(transfer.requeue_call(held).unwrap_err(), E::Full);
    assert!(
        matches!(
            futures_poll(original.as_mut()).await,
            Some(Err(flume::RecvError::Disconnected))
        ),
        "the original caller must disconnect without canceling itself"
    );
    let queued: Vec<_> = transfer.service_drain()?.collect();
    assert_eq!(queued.len(), 10);
    assert!(queued.iter().all(|call| call.kind() == "GenerateUserCreds"));
    Ok(())
}

#[tokio::test]
async fn requeue_losing_to_claim_releases_original_waiter_and_cannot_enter_after_close()
-> Result<()> {
    let transfer = Transfer::new();
    let mut original = pin!(transfer.send_to_service_and_get(request("held-original")));
    assert!(futures_poll(original.as_mut()).await.is_none());
    let held = transfer.service_drain()?.next().context("held envelope")?;
    let run = transfer.claim_run()?;
    assert_eq!(transfer.requeue_call(held).unwrap_err(), E::AlreadyOwned);
    assert!(matches!(
        futures_poll(original.as_mut()).await,
        Some(Err(flume::RecvError::Disconnected))
    ));
    assert!(matches!(transfer.service_drain(), Err(E::AlreadyOwned)));
    assert!(
        futures_poll(pin!(run.receive_call())).await.is_none(),
        "no rejected diagnostic request may enter active queue"
    );
    drop(run);
    assert!(matches!(transfer.service_drain(), Err(E::Closed)));
    Ok(())
}

#[tokio::test]
async fn close_winning_before_requeue_releases_same_original_sender_without_reopening() -> Result<()>
{
    let transfer = Transfer::new();
    let mut original = pin!(transfer.send_to_service_and_get(request("held-original")));
    assert!(futures_poll(original.as_mut()).await.is_none());
    let held = transfer.service_drain()?.next().context("held envelope")?;
    drop(transfer.claim_run()?);
    assert_eq!(transfer.requeue_call(held).unwrap_err(), E::Closed);
    assert!(matches!(
        futures_poll(original.as_mut()).await,
        Some(Err(flume::RecvError::Disconnected))
    ));
    assert!(matches!(transfer.claim_run(), Err(E::Closed)));
    Ok(())
}

#[tokio::test]
async fn dropping_drained_envelope_disconnects_its_original_caller_without_replacement()
-> Result<()> {
    let transfer = Transfer::new();
    let mut original = pin!(transfer.send_to_service_and_get(request("held-original")));
    assert!(futures_poll(original.as_mut()).await.is_none());
    let held: Vec<_> = transfer.service_drain()?.collect();
    assert_eq!(held.len(), 1);
    drop(held);
    assert!(matches!(
        futures_poll(original.as_mut()).await,
        Some(Err(flume::RecvError::Disconnected))
    ));
    let run = transfer.claim_run()?;
    assert!(futures_poll(pin!(run.receive_call())).await.is_none());
    Ok(())
}
