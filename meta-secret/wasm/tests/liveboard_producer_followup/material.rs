//! Independent proof of resolved local lost reply versus genuine later partial material.
use super::{
    context_repo::{Fault, Repo},
    fixture::*,
};
use anyhow::{Context, Result};
use meta_secret_core::node::{
    common::model::liveboard_producer::*,
    db::{
        descriptors::shared_secret_descriptor::{SsDeviceLogDescriptor, SsLogDescriptor},
        in_mem_db::InMemKvLogEventRepo,
    },
};
use std::sync::{Arc, atomic::Ordering};

async fn run(fault: Fault, resolved: bool) -> Result<()> {
    let inner = Arc::new(Repo::new(Arc::new(InMemKvLogEventRepo::default())));
    let h = Harness::new(inner.clone(), 2).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        inner.reset();
        inner.arm(fault);
        let receipt = h.remove(true).await?;
        assert_eq!(inner.hits.load(Ordering::SeqCst), 1);
        let actual = h.p.repo.sources();
        assert_eq!(actual.len(), 2);
        assert_source(&receipt, first_request(&actual)?)?;
        let Some(ProducerDecisionStage::Attempted { source, .. }) = &receipt.decision_stage else {
            anyhow::bail!("actual U identity");
        };
        assert_eq!(source.event_id, actual[1].0.key.obj_id);
        let local =
            h.p.find_tail_event(SsDeviceLogDescriptor::from(h.actor.device_id().clone()))
                .await?
                .context("actual retained singleton local material")?;
        assert!(local.0.value.receivers.is_empty());
        let canonical =
            h.p.find_tail_event(SsLogDescriptor::from(h.actor.vault_name.clone()))
                .await?
                .context("actual canonical material stage")?;
        let relevant: Vec<_> = canonical
            .0
            .value
            .claims
            .values()
            .filter(|claim| {
                claim.dist_claim_id.pass_id == h.primary
                    || claim.dist_claim_id.pass_id == h.secondary
            })
            .collect();
        assert_eq!(relevant.len(), 2);
        let singleton = relevant
            .iter()
            .filter(|c| c.sender == *h.actor.device_id() && c.receivers.is_empty())
            .count();
        if resolved {
            assert!(
                inner.resolved_readbacks.load(Ordering::SeqCst) > 0,
                "unchanged helper must resolve exact persisted local body"
            );
            assert_eq!(
                singleton, 2,
                "both actual canonical Secret claims completed"
            );
            assert_eq!(receipt.material_stage, Some(ProducerMaterialStage::Ready));
            // Overall canonical/receipt readiness remains its separately computed contract.
        } else {
            assert_eq!(
                singleton, 1,
                "real first canonical save persisted; second Secret did not finish"
            );
            assert_eq!(inner.singleton_canonical_saves.load(Ordering::SeqCst), 1);
            assert_eq!(receipt.phase, ProducerPhase::Unknown);
            assert!(matches!(
                receipt.material_stage,
                Some(ProducerMaterialStage::Pending | ProducerMaterialStage::Unknown)
            ));
            assert!(!["ready", "done", "settled"].contains(&receipt.readiness.as_str()));
        }
        let writes = inner.writes.load(Ordering::SeqCst);
        let business = h.network.counts().0;
        let observed = h.reconcile(false, reference(&receipt)).await?;
        assert_eq!(observed.request_stage.source, receipt.request_stage.source);
        assert_eq!(observed.decision_stage, receipt.decision_stage);
        assert_eq!(inner.writes.load(Ordering::SeqCst), writes);
        assert_eq!(h.network.counts().0, business);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn exact_local_material_lost_reply_resolves_via_actual_readback_positive_control()
-> Result<()> {
    run(Fault::ResolvedMaterialLostReply, true).await
}
#[tokio::test]
async fn downstream_actual_canonical_lost_reply_preserves_real_partial_material_control()
-> Result<()> {
    run(Fault::CanonicalMaterialLostReply, false).await
}
