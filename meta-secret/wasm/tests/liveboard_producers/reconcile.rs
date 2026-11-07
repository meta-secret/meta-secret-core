use super::fixture::*;
use anyhow::{Context, Result};
use meta_secret_core::node::{
    common::model::liveboard_producer::*, db::in_mem_db::InMemKvLogEventRepo,
};
use std::sync::{Arc, atomic::Ordering};

async fn read_only(mobile: bool) -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    drive(&h, async {
        h.web.get_state().await?;
        let receipt = h
            .delete(mobile, h.primary.clone())
            .await
            .context("RED: actual producer required before read-only reconciliation")?;
        let input = reference(&receipt);
        let writes = h.p.repo.writes.load(Ordering::SeqCst);
        let network = h.network.counts();
        let sources = h.p.repo.sources();
        let observed = h
            .reconcile(mobile, input.clone())
            .await
            .context("typed actual read-only producer observation")?;
        assert_eq!(observed.request_stage.source, input.request_stage);
        assert_eq!(observed.request_id, input.request_id);
        assert_eq!(
            h.p.repo.writes.load(Ordering::SeqCst),
            writes,
            "reconcile must not append/replace/delete/materialize"
        );
        let after = h.network.counts();
        assert_eq!(after.0, network.0, "reconcile must not upload/sign/flush");
        assert_eq!(
            after.2, network.2,
            "reconcile must not call ordinary sync/get_app_state"
        );
        assert_eq!(
            after.1,
            network.1 + 1,
            "one actual atomic ServerTail observation"
        );
        assert!(
            h.p.repo.sources() == sources,
            "reconcile cannot reserve a replacement R/U"
        );
        Ok(())
    })
    .await
}
#[tokio::test]
async fn web_reconcile_reads_only_one_atomic_observation_without_client_sync_or_mutation()
-> Result<()> {
    read_only(false).await
}
#[tokio::test]
async fn mobile_reconcile_reads_only_one_atomic_observation_without_client_sync_or_mutation()
-> Result<()> {
    read_only(true).await
}

#[tokio::test]
async fn changed_reference_cannot_become_authority_or_trigger_a_replacement_producer() -> Result<()>
{
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    drive(&h, async {
        h.web.get_state().await?;
        let receipt = h
            .remove(false)
            .await
            .context("RED: exact actual source prerequisite")?;
        let mut changed = reference(&receipt);
        changed.request_stage.body_commitment = "00".repeat(32);
        let writes = h.p.repo.writes.load(Ordering::SeqCst);
        let network = h.network.counts();
        let sources = h.p.repo.sources();
        let result = h.reconcile(true, changed).await;
        if let Ok(value) = result {
            assert_eq!(value.phase, ProducerPhase::Unknown);
            assert!(!matches!(
                value.canonical_state,
                ProducerCanonicalState::Applied | ProducerCanonicalState::Declined
            ));
        }
        assert_eq!(h.p.repo.writes.load(Ordering::SeqCst), writes);
        assert_eq!(h.network.counts().0, network.0);
        assert!(h.p.repo.sources() == sources);
        Ok(())
    })
    .await
}
