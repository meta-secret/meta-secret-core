use super::{
    repo::{Fault, Point},
    support::*,
    *,
};
#[tokio::test]
async fn s05_fixed_witness_faults_no_partial_body_and_exact_lost_reply_readback() -> Result<()> {
    let mut results = vec![];
    for fault in [
        Fault::Before,
        Fault::Drop,
        Fault::Readback,
        Fault::LostReply,
    ] {
        results.push(run(fault).await);
    }
    for r in results {
        r?;
    }
    Ok(())
}
async fn run(fault: Fault) -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let before = h.p.repo.snapshot().await?;
    h.p.repo.arm(Point::Witness, fault);
    let r = h.decide().await;
    eprintln!(
        "C1b fixed-witness {fault:?} hits={} readback_faults={}",
        h.p.repo.hits.load(Ordering::SeqCst),
        h.p.repo.read_hits.load(Ordering::SeqCst)
    );
    ensure!(
        h.p.repo.hits.load(Ordering::SeqCst) > 0,
        "private witness fault boundary UNREACHED"
    );
    if fault == Fault::LostReply {
        let receipt = r?;
        ensure!(
            h.p.repo.inner.find_one(receipt.event_id).await?.is_some(),
            "resolved queued receipt has no actual body"
        );
        super::stage::private_crypto(&h.state, &h.witness().await?)?;
    } else {
        static_error(&r)?;
        h.no_body_or_send().await?;
        if fault == Fault::Readback {
            ensure!(
                h.p.repo.read_hits.load(Ordering::SeqCst) > 0,
                "write succeeded but readback fault not exercised"
            );
        }
    }
    ensure!(
        ordinary(&h.p.repo.snapshot().await?) == ordinary(&before),
        "private witness failure published ordinary material"
    );
    ensure!(
        h.p.repo
            .inner
            .find_one(h.state.witness_id().next())
            .await?
            .is_none(),
        "private store appended packet history"
    );
    Ok(())
}
