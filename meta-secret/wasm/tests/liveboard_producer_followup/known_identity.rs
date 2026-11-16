//! Known durable exact UUID cannot be overwritten by a contradictory valid caller hint.
use super::{
    context_repo::{Fault, Repo},
    fixture::*,
    terminal,
};
use anyhow::{Context, Result};
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use std::sync::{Arc, atomic::Ordering};
#[tokio::test]
async fn contradictory_known_request_uuid_fails_closed_before_atomic_query_or_local_write()
-> Result<()> {
    let backend = Arc::new(InMemKvLogEventRepo::default());
    let saved = terminal::interrupt(backend.clone(), Fault::ClearBefore).await?;
    let known = saved
        .context
        .request_id
        .clone()
        .context("actual assigned canonical request UUID")?;
    let mut input = reference(&saved.receipt);
    // Deliberately contradictory INPUT, never a fabricated request/source fixture.
    let wrong = "00000000-0000-4000-8000-000000000001".to_string();
    assert_ne!(known, wrong);
    input.request_id = Some(wrong);
    let original_r = input.request_stage.clone();
    let original_u = input.decision_stage.clone();
    let h = Harness::reopen(Arc::new(Repo::new(backend.clone())), saved.state)?;
    // Positive method-boundary control: one REAL atomic query, zero local mutation/clear.
    let control_db = backend.get_db().await;
    let control_context = terminal::context(&h).await?;
    let control_network = h.network.counts();
    let control_writes = h.p.repo.inner.writes.load(Ordering::SeqCst);
    let observed = h
        .service
        .reconcile_producer_receipt(reference(&saved.receipt))
        .await?;
    assert_eq!(observed.request_id.as_ref(), Some(&known));
    assert_eq!(observed.request_stage.source, original_r);
    assert!(backend.get_db().await == control_db);
    assert!(terminal::context(&h).await? == control_context);
    assert_eq!(h.p.repo.inner.writes.load(Ordering::SeqCst), control_writes);
    assert_eq!(
        h.network.counts(),
        (control_network.0, control_network.1 + 1, control_network.2)
    );
    let before = backend.get_db().await;
    let context = terminal::context(&h).await?;
    let writes = h.p.repo.inner.writes.load(Ordering::SeqCst);
    let network = h.network.counts();
    let outcome = h.service.reconcile_producer_receipt(input).await;
    assert!(
        before == backend.get_db().await,
        "contradictory identity must preserve DB"
    );
    assert!(terminal::context(&h).await? == context);
    assert_eq!(h.p.repo.inner.writes.load(Ordering::SeqCst), writes);
    assert!(h.p.repo.sources().is_empty(), "no new business R/U upload");
    let after = h.network.counts();
    eprintln!(
        "KNOWN_ID_CONTRADICTION actual_context_uuid=true same_r_u=true query_delta={} result_ok={}",
        after.1 - network.1,
        outcome.is_ok()
    );
    assert_eq!(
        after, network,
        "RED: known contradictory UUID must fail before atomic query"
    );
    assert!(
        outcome.is_err(),
        "RED: reconcile must not replace caller's contradictory UUID with durable UUID"
    );
    let completed = drive(&h, h.resume(false, reference(&saved.receipt))).await?;
    assert_eq!(completed.request_id.as_ref(), Some(&known));
    assert_eq!(completed.request_stage.source, original_r);
    assert!(
        matches!(&completed.decision_stage,Some(meta_secret_core::node::common::model::liveboard_producer::ProducerDecisionStage::Attempted{source,..}) if Some(source)==original_u.as_ref())
    );
    assert!(terminal::context(&h).await?.is_none());
    Ok(())
}
