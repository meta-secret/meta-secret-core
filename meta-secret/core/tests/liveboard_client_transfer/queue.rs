use super::*;
use anyhow::{Context, Result};
use meta_secret_core::node::app::meta_app::client_transfer::MetaClientRequestTransfer as Transfer;
use std::pin::pin;

#[tokio::test]
async fn simultaneous_live_checked_and_legacy_waiters_keep_each_reply_and_fifo_kind() -> Result<()>
{
    let transfer = Transfer::new();
    let mut a = pin!(transfer.send_to_service_and_get(request("first")));
    let mut b = pin!(transfer.send_to_service_and_get(Q::GetState));
    let mut c = pin!(transfer.send_to_service_and_get(request("third")));
    for future in [&mut a, &mut b, &mut c] {
        assert!(
            futures_poll(future.as_mut()).await.is_none(),
            "RED: live request must enqueue and wait for its own reply"
        );
    }
    let run = transfer.claim_run()?;
    for (kind, value) in [
        ("ClusterDistributionChecked", receipt("first-id")),
        ("GetState", A::Error("legacy-only-error".into())),
        ("ClusterDistributionChecked", receipt("third-id")),
    ] {
        let call = run.receive_call().await?;
        assert_eq!(call.kind(), kind);
        let (_, reply) = call.into_parts();
        reply.context("live caller reply target")?.respond(value);
    }
    assert_receipt(a.await?, "first-id");
    assert!(matches!(b.await?, A::Error(e) if e == "legacy-only-error"));
    assert_receipt(c.await?, "third-id");
    Ok(())
}

pub async fn futures_poll<F: std::future::Future>(
    mut future: std::pin::Pin<&mut F>,
) -> Option<F::Output> {
    std::future::poll_fn(|cx| {
        std::task::Poll::Ready(match future.as_mut().poll(cx) {
            std::task::Poll::Ready(value) => Some(value),
            std::task::Poll::Pending => None,
        })
    })
    .await
}

#[tokio::test]
async fn canceled_sender_observed_behind_full_ten_slot_queue_never_enters_it() -> Result<()> {
    let transfer = Transfer::new();
    for index in 0..10 {
        transfer.send_to_service(fire(index)).await;
    }
    let mut canceled = Box::pin(transfer.send_to_service_and_get(request("never-admitted")));
    assert!(futures_poll(canceled.as_mut()).await.is_none());
    drop(canceled);
    let queued: Vec<_> = transfer.service_drain()?.collect();
    assert_eq!(
        queued.len(),
        10,
        "full queue must contain only its previously accepted ten commands"
    );
    for call in queued {
        transfer.requeue_call(call)?;
    }
    let run = transfer.claim_run()?;
    for index in 0..10 {
        let (request, reply) = run.receive_call().await?.into_parts();
        assert!(matches!(request, Q::GenerateUserCreds(v) if v.0 == format!("local-{index}")));
        assert!(reply.is_none());
    }
    assert!(
        futures_poll(pin!(run.receive_call())).await.is_none(),
        "never accepted canceled command must not exist"
    );
    Ok(())
}

#[tokio::test]
async fn accepted_then_canceled_envelope_keeps_one_command_without_poisoning_live_reply()
-> Result<()> {
    let transfer = Transfer::new();
    let mut canceled = Box::pin(transfer.send_to_service_and_get(request("accepted-canceled")));
    assert!(futures_poll(canceled.as_mut()).await.is_none());
    let mut held = transfer.service_drain()?;
    let call = held.next().context("accepted original envelope")?;
    assert!(held.next().is_none());
    drop(canceled);
    transfer.requeue_call(call)?;
    let run = transfer.claim_run()?;
    let (_, abandoned_reply) = run.receive_call().await?.into_parts();
    abandoned_reply
        .context("original reply sender retained")?
        .respond(receipt("abandoned-id"));
    let mut live = pin!(transfer.send_to_service_and_get(request("live")));
    assert!(futures_poll(live.as_mut()).await.is_none());
    let (_, reply) = run.receive_call().await?.into_parts();
    reply.context("live target")?.respond(receipt("live-id"));
    assert_receipt(live.await?, "live-id");
    assert!(futures_poll(pin!(run.receive_call())).await.is_none());
    Ok(())
}

#[tokio::test]
async fn more_fire_and_forget_than_old_response_capacity_then_correlated_call_never_stalls()
-> Result<()> {
    let transfer = Transfer::new();
    let run = transfer.claim_run()?;
    let producers = async {
        for index in 0..24 {
            transfer.send_to_service(fire(index)).await;
        }
        transfer
            .send_to_service_and_get(request("after-fire-forget"))
            .await
    };
    let consumer = async {
        for index in 0..24 {
            let (request, reply) = run.receive_call().await?.into_parts();
            assert!(matches!(request, Q::GenerateUserCreds(v) if v.0 == format!("local-{index}")));
            assert!(reply.is_none(), "fire-and-forget must not allocate a reply");
        }
        let (_, reply) = run.receive_call().await?.into_parts();
        reply
            .context("correlated target")?
            .respond(receipt("last-id"));
        anyhow::Ok(())
    };
    let (result, completed) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(producers, consumer)
    })
    .await
    .context("fire-and-forget transport watchdog elapsed")?;
    completed?;
    assert_receipt(result?, "last-id");
    Ok(())
}
