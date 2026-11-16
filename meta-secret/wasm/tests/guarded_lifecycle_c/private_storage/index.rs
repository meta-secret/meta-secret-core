use super::{
    repo::{Fault, Point},
    support::*,
    *,
};
#[tokio::test]
async fn s03_changed_index_replace_before_drop_readback_is_required_before_payload() -> Result<()> {
    let mut results = vec![];
    results.push(unsupported().await);
    for fault in [Fault::Before, Fault::Drop, Fault::Readback] {
        results.push(changed(fault).await);
    }
    for r in results {
        r?;
    }
    Ok(())
}
async fn unsupported() -> Result<()> {
    let h = H::new(Arc::new(super::repo::Unsupported(Arc::new(
        InMemKvLogEventRepo::default(),
    ))))
    .await?;
    let before = h.p.repo.snapshot().await?;
    let result = h.decide().await;
    let reached=h.p.repo.trace.lock().unwrap().iter().any(|t|matches!(t,super::repo::Trace::Write(true,event) if matches!(event.as_ref(),GenericKvLogEvent::LocalGuardedPreparationIndex(x) if x.0.value.revision>0)));
    eprintln!("C1b trait-default unsupported changed_index_reached={reached}");
    ensure!(reached, "unsupported changed-index boundary UNREACHED");
    ensure!(result.is_err(), "unsupported replace allowed payload/queue");
    h.no_body_or_send().await?;
    ensure!(
        ordinary(&h.p.repo.snapshot().await?) == ordinary(&before),
        "unsupported replace published ordinary material"
    );
    ensure!(
        h.p.repo
            .inner
            .find_one(h.state.witness_id())
            .await?
            .is_none(),
        "unsupported backend got cipher witness"
    );
    Ok(())
}
async fn changed(fault: Fault) -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let before = h.p.repo.snapshot().await?;
    h.p.repo.arm(Point::Index, fault);
    let r = h.decide().await;
    eprintln!(
        "C1b changed-index {fault:?} hits={}",
        h.p.repo.hits.load(Ordering::SeqCst)
    );
    ensure!(
        h.p.repo.hits.load(Ordering::SeqCst) > 0,
        "changed-index replace fault UNREACHED"
    );
    if fault == Fault::Readback {
        ensure!(
            h.p.repo.read_hits.load(Ordering::SeqCst) > 0,
            "changed-index readback fault UNREACHED"
        );
    }
    static_error(&r)?;
    h.no_body_or_send().await?;
    ensure!(
        ordinary(&h.p.repo.snapshot().await?) == ordinary(&before),
        "index replace uncertainty published ordinary material"
    );
    ensure!(
        h.p.repo
            .inner
            .find_one(h.state.witness_id())
            .await?
            .is_none(),
        "encrypted witness saved without changed replace proof"
    );
    Ok(())
}
#[tokio::test]
async fn s04_same_uuid_index_without_witness_requires_new_changed_probe_then_exact_retry()
-> Result<()> {
    let mut results = vec![];
    for fault in [
        Fault::Before,
        Fault::Drop,
        Fault::Readback,
        Fault::LostReply,
    ] {
        results.push(retry(fault).await);
    }
    for r in results {
        r?;
    }
    Ok(())
}
async fn retry(fault: Fault) -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    h.p.repo.arm(Point::Witness, Fault::Before);
    let _first = h.decide().await;
    ensure!(
        h.p.repo.hits.load(Ordering::SeqCst) > 0,
        "actual index-committed/witness-absent crash boundary UNREACHED"
    );
    let original = h.index().await?;
    ensure!(
        h.p.repo
            .inner
            .find_one(h.state.witness_id())
            .await?
            .is_none(),
        "crash unexpectedly stored payload"
    );
    h.no_body_or_send().await?;
    h.p.repo.arm(Point::Index, fault);
    let r = h.decide().await;
    ensure!(
        h.p.repo.hits.load(Ordering::SeqCst) > 0,
        "sameUUID repeat changed-index probe bypassed"
    );
    if fault == Fault::Readback {
        ensure!(
            h.p.repo.read_hits.load(Ordering::SeqCst) > 0,
            "sameUUID changed-index readback fault UNREACHED"
        );
    }
    if fault != Fault::LostReply {
        static_error(&r)?;
        h.no_body_or_send().await?;
        ensure!(
            h.p.repo
                .inner
                .find_one(h.state.witness_id())
                .await?
                .is_none(),
            "repeat probe failure saved payload"
        );
        h.p.repo.clear();
    }
    let _healthy = if fault == Fault::LostReply {
        r?
    } else {
        h.decide().await?
    };
    let current = h.index().await?;
    ensure!(
        current.0.value.revision > original.0.value.revision
            && current.0.value.request_ids == vec![h.state.uuid.clone()],
        "sameUUID retry duplicated ID or failed to change revision"
    );
    super::stage::private_crypto(&h.state, &h.witness().await?)?;
    Ok(())
}
#[tokio::test]
async fn s08_invalid_index_owner_duplicate_uuid_overflow_and_duplicate_attempt_fail_closed()
-> Result<()> {
    let mut results = vec![];
    for kind in 0..4 {
        results.push(invalid(kind).await);
    }
    for r in results {
        r?;
    }
    Ok(())
}
async fn invalid(kind: u8) -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    // Labelled local corruption. A syntactically valid actual UUID is unchanged.
    let mut index = LocalGuardedPreparationIndexObject(KvLogEvent {
        key: KvKey::from(LocalGuardedPreparationIndexDescriptor {
            owner: h.state.b.user_id(),
        }),
        value: LocalGuardedPreparationIndex {
            version: 1,
            owner: h.state.b.user_id(),
            request_ids: vec![h.state.uuid.clone()],
            revision: 1,
        },
    });
    match kind {
        0 => index.0.value.owner = h.state.a.user_id(),
        1 => index.0.value.request_ids.push(h.state.uuid.clone()),
        2 => index.0.value.revision = u64::MAX,
        _ => {
            let _receipt = h.decide().await?;
            let mut w = h.witness().await?;
            let o =
                w.0.value
                    .origin
                    .clone()
                    .context("real actual origin for duplicate fixture")?;
            w.0.value.previous_attempts.push(GuardedCompactAttempt {
                origin: o,
                transport_noop: None,
                phase: w.0.value.phase,
                base_vault: w.0.value.base_vault.clone(),
                requester_id: w.0.value.requester_id.clone(),
                target_id: w.0.value.target_id.clone(),
                survivors: w.0.value.survivors.clone(),
                claims: vec![],
                packets: vec![],
            });
            h.p.repo.inner.replace(w).await?;
        }
    }
    if kind < 3 {
        h.p.repo.save(index).await?;
    }
    let before = h.p.repo.snapshot().await?;
    let r = h.decide().await;
    ensure!(r.is_err(), "invalid local index/attempt record was ignored");
    ensure!(
        h.p.repo.snapshot().await? == before,
        "invalid local metadata caused mutation"
    );
    ensure!(
        h.wire.writes.load(Ordering::SeqCst) == 0,
        "invalid metadata reached transport"
    );
    Ok(())
}
