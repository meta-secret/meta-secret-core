use super::*;

#[tokio::test]
async fn auth_before_drop_persistent_readback_and_durable_lost_reply_preserve_privacy_and_exact_retry()
-> Result<()> {
    let mut results = vec![];
    for mode in [Mode::Before, Mode::Drop, Mode::ReadbackErr, Mode::LostReply] {
        results.push(run(mode).await);
    }
    for result in results {
        result?;
    }
    Ok(())
}
async fn run(mode: Mode) -> Result<()> {
    eprintln!("AUTH4 {mode:?} START fresh actual FULL2 queued N and real pending tombstone");
    let h = harness::create(mode).await?;
    let cuts = h.wire.cuts.lock().unwrap().len();
    let first = h.gateway.sync(h.case.h.state.b.user()).await;
    let hits = h.repo.hits.load(Ordering::SeqCst);
    let read_hits = h.repo.read_hits.load(Ordering::SeqCst);
    let no_send =
        h.wire.noops.lock().unwrap().is_empty() && h.wire.actions.lock().unwrap().is_empty();
    let returned_auth_error = first
        .as_ref()
        .err()
        .is_some_and(|e| e.to_string() == "GuardedPreparationUnavailable");
    let first_proof = if mode == Mode::LostReply {
        proof::settled(&h, &first, cuts).await
    } else {
        let privacy = proof::privacy(&h).await;
        ensure_observed_error(returned_auth_error, no_send, hits, read_hits, mode).and(privacy)
    };
    let actual = h.repo.witness().await?;
    let auth_durable = actual.0.value.transport_noop.is_some();
    let physical_full = actual.0.value.payload.is_some();
    eprintln!(
        "AUTH4 {mode:?} FIRST actual_auth_write_hits={hits} actual_failed_readback_hits={read_hits} returned_auth_error={returned_auth_error} no_send={no_send} physical_FULL_retained={physical_full} physical_auth_durable={auth_durable} first_oracles_ok={}",
        first_proof.is_ok()
    );
    ensure!(
        hits == 1,
        "auth4 actual first intended transition fault UNREACHED"
    );
    h.repo.clear();
    let cuts = h.wire.cuts.lock().unwrap().len();
    let retry = h.gateway.sync(h.case.h.state.b.user()).await;
    let retry_proof = proof::settled(&h, &retry, cuts).await;
    eprintln!(
        "AUTH4 {mode:?} CLEARED_RETRY actual_typed_SS_boundary={} actual_Noop_calls={} ever_durable_auth_same_and_real_receipt_tail_ok={}",
        h.wire.stopped(&retry, cuts),
        h.wire.noops.lock().unwrap().len(),
        retry_proof.is_ok()
    );
    first_proof?;
    retry_proof?;
    Ok(())
}
fn ensure_observed_error(
    returned: bool,
    no_send: bool,
    hits: usize,
    reads: usize,
    mode: Mode,
) -> Result<()> {
    ensure!(
        returned && no_send && hits == 1,
        "actual auth error path sent transport or leaked/changed public error"
    );
    if mode == Mode::ReadbackErr {
        ensure!(
            reads >= 1,
            "actual intended persistent auth readback boundary UNREACHED"
        );
    }
    Ok(())
}
