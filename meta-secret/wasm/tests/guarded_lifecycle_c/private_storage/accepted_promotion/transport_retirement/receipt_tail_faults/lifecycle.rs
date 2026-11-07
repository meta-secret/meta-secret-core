use super::*;

#[tokio::test]
async fn seven_real_receipt_tail_cuts_reuse_exact_authorization_receipt_and_confirmed_tail()
-> Result<()> {
    let mut results = vec![];
    for mode in [
        Mode::ServerReplyLoss,
        Mode::ReceiptBefore,
        Mode::ReceiptDrop,
        Mode::ReceiptReadback,
        Mode::ReceiptLostReply,
        Mode::TailLoss,
        Mode::ConfirmedReadback,
    ] {
        results.push(run(mode).await);
    }
    for result in results {
        result?;
    }
    Ok(())
}
async fn run(mode: Mode) -> Result<()> {
    eprintln!("RECEIPT_TAIL {mode:?} START fresh actual FULL2/tombstone");
    let h = harness::Harness::new(Arc::new(InMemKvLogEventRepo::default()), mode).await?;
    let cuts = h.wire.cuts.lock().unwrap().len();
    let first = h.sync().await;
    let first_proof = if mode == Mode::ReceiptLostReply {
        proof::settled(&h, &first, cuts).await
    } else {
        proof::interrupted(&h, mode, &first).await
    };
    let hits = if mode.repo_fault() {
        h.repo.hits.load(Ordering::SeqCst)
    } else {
        h.wire.hits.load(Ordering::SeqCst)
    };
    let reads = h.repo.read_hits.load(Ordering::SeqCst);
    let w = h.repo.witness().await?;
    let technical = w.0.value.transport_noop.as_ref();
    eprintln!(
        "RECEIPT_TAIL {mode:?} FIRST actual_fault_hits={hits} intended_failed_readbacks={reads} returned_error={} receipt_durable={} confirmed_durable={} first_oracles_ok={}",
        first.is_err(),
        technical.is_some_and(|n| n.receipt.is_some()),
        technical.is_some_and(|n| n.confirmed_next_tail.is_some()),
        first_proof.is_ok()
    );
    let gate = hits == 1
        && (!matches!(mode, Mode::ReceiptReadback | Mode::ConfirmedReadback) || reads == 1);
    h.clear();
    let cuts = h.wire.cuts.lock().unwrap().len();
    let retry = h.sync().await;
    let retry_proof = proof::settled(&h, &retry, cuts).await;
    let before = h.repo.witness().await?;
    let noops = h.wire.noops.lock().unwrap().len();
    let cuts = h.wire.cuts.lock().unwrap().len();
    let repeat = h.sync().await;
    let repeat_proof = proof::settled(&h, &repeat, cuts).await;
    let stable = h.repo.witness().await? == before && h.wire.noops.lock().unwrap().len() == noops;
    let actual_receipts = h
        .wire
        .noops
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.returned)
        .count();
    let retained_orders = h
        .repo
        .writes
        .lock()
        .unwrap()
        .iter()
        .map(|w| w.order)
        .collect::<Vec<_>>();
    ensure!(
        retained_orders.windows(2).all(|w| w[0] < w[1]),
        "outer write ordering trace is inconsistent"
    );
    eprintln!(
        "RECEIPT_TAIL {mode:?} RETRY actual_Noop_attempts={noops} actual_receipts_returned={actual_receipts} retry_exact_proof={} repeat_exact_proof={} confirmed_repeat_no_resend_and_immutable={stable}",
        retry_proof.is_ok(),
        repeat_proof.is_ok()
    );
    ensure!(
        gate,
        "actual selected changed receipt/tail boundary UNREACHED"
    );
    first_proof?;
    retry_proof?;
    repeat_proof?;
    ensure!(
        stable,
        "healthy confirmed repeat changed proof or resent Noop"
    );
    Ok(())
}
