use super::{
    fixture::{Harness, ordinary},
    network::Capability,
};
use anyhow::{Context, Result, ensure};
use meta_secret_core::node::{
    common::model::liveboard::LiveboardOperationPhase, db::repo::generic_db::FindOneQuery,
};
use std::sync::atomic::Ordering;

async fn rejected_without_mutation(mode: Capability) -> Result<()> {
    let h = Harness::new().await?;
    let before = h.p.repo.get_db().await;
    let server_before = h.server_repo.get_db().await;
    h.network.arm(mode);
    let result = h.gateway().decide_remove_member(h.input()).await;
    eprintln!(
        "C1 capability mode={} probes={} rejected={} local_unchanged={} server_unchanged={} writes={}",
        mode as u8,
        h.network.probes.load(Ordering::SeqCst),
        result.is_err(),
        h.p.repo.get_db().await == before,
        h.server_repo.get_db().await == server_before,
        h.network.writes.load(Ordering::SeqCst)
    );
    ensure!(
        h.network.probes.load(Ordering::SeqCst) == 1,
        "actual capability boundary not reached"
    );
    ensure!(
        result.is_err(),
        "unsupported/failed Noop capability must reject before preparation or queue"
    );
    ensure!(
        h.p.repo.get_db().await == before,
        "capability failure mutated local preparation/origin/body/material"
    );
    ensure!(
        h.server_repo.get_db().await == server_before,
        "capability-only failure mutated canonical server state"
    );
    ensure!(
        h.network.writes.load(Ordering::SeqCst) == 0
            && h.network.accepts.lock().unwrap().is_empty(),
        "capability failure sent a business/technical write"
    );
    h.assert_original_k2(&before)?;
    Ok(())
}
#[tokio::test]
async fn noop_zero_with_projection_one_guarded_one_prevents_all_preparation_and_queue_mutation()
-> Result<()> {
    rejected_without_mutation(Capability::NoopAbsent).await
}
#[tokio::test]
async fn capability_transport_failure_prevents_all_preparation_and_queue_mutation() -> Result<()> {
    rejected_without_mutation(Capability::TransportFailure).await
}
#[tokio::test]
async fn two_secrets_keep_original_live_k2_and_ordinary_claims_at_checked_gateway_and_transport_send()
-> Result<()> {
    let h = Harness::new().await?;
    let before = h.p.repo.get_db().await;
    let server_before = h.server_repo.get_db().await;
    let gateway = h.gateway();
    let receipt = gateway.decide_remove_member(h.input()).await?;
    ensure!(
        receipt.phase == LiveboardOperationPhase::Queued && receipt.request_id == h.request_id,
        "healthy complete stage must queue the actual canonical request"
    );
    ensure!(
        h.p.repo.find_one(receipt.event_id.clone()).await?.is_some(),
        "queued receipt has no actual durable body"
    );
    ensure!(
        h.network.writes.load(Ordering::SeqCst) == 0,
        "checked gateway sent before ordinary flush"
    );
    ensure!(
        h.server_repo.get_db().await == server_before,
        "pre-send gateway changed server canonical state"
    );
    let after_gateway = h.p.repo.get_db().await;
    // Reach actual ordinary send even if the earlier snapshot was already wrong.
    // Later sync completion is outside this selected pre-send boundary assertion.
    let sync_outcome = gateway.sync(h.b.user()).await;
    let boundaries = h.network.accepts.lock().unwrap().clone();
    let first = boundaries
        .first()
        .context("actual guarded Accept transport boundary not reached")?;
    eprintln!(
        "C1 pre-send secrets=2 boundaries={} gateway_ordinary_unchanged={} send_ordinary_unchanged={} gateway_k2_valid={} send_k2_valid={} actual_sync_ok={}",
        boundaries.len(),
        ordinary(&after_gateway) == ordinary(&before),
        boundaries
            .iter()
            .all(|snapshot| ordinary(snapshot) == ordinary(&before)),
        h.assert_original_k2(&after_gateway).is_ok(),
        boundaries
            .iter()
            .all(|snapshot| h.assert_original_k2(snapshot).is_ok()),
        sync_outcome.is_ok()
    );
    ensure!(
        ordinary(&after_gateway) == ordinary(&before),
        "private preparation replaced original live packets or appended ordinary claims before send"
    );
    h.assert_original_k2(&after_gateway)?;
    for snapshot in &boundaries {
        ensure!(
            ordinary(snapshot) == ordinary(&before),
            "Accept send boundary contains prematurely published material/claims"
        );
        h.assert_original_k2(snapshot)?;
    }
    ensure!(
        ordinary(first) == ordinary(&before),
        "actual send snapshot must preserve both Secrets"
    );
    sync_outcome.context("actual ordinary sync after guarded send")?;
    Ok(())
}
