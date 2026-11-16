//! Actual completed nonempty2→1: caller U/UUID survive only as exact read-only hints.
use super::{context_repo::Repo, fixture::*, terminal};
use anyhow::{Context, Result};
use meta_secret_core::node::{
    common::model::liveboard_producer::*,
    db::{
        descriptors::vault_descriptor::VaultDescriptor,
        events::{generic_log_event::*, object_id::Next},
        in_mem_db::InMemKvLogEventRepo,
        objects::persistent_object::PersistentObject,
        repo::generic_db::*,
    },
};
use std::sync::{Arc, atomic::Ordering};
type Local = Repo<InMemKvLogEventRepo>;
struct Completed {
    h: Harness<Local>,
    receipt: LiveboardProducerReceipt,
}
impl Completed {
    async fn new() -> Result<Self> {
        let h = Harness::new(
            Arc::new(Repo::new(Arc::new(InMemKvLogEventRepo::default()))),
            2,
        )
        .await?;
        let receipt = drive(&h, async {
            h.web.get_state().await?;
            h.p.repo.reset_observation();
            h.remove(false).await
        })
        .await?;
        assert_eq!(receipt.phase, ProducerPhase::Queued);
        assert_eq!(receipt.canonical_state, ProducerCanonicalState::Applied);
        assert_eq!(receipt.material_stage, Some(ProducerMaterialStage::Ready));
        assert!(["ready", "done", "settled"].contains(&receipt.readiness.as_str()));
        assert!(
            terminal::context(&h).await?.is_none(),
            "actual successful lifecycle cleared its context"
        );
        let sources = h.p.repo.sources();
        assert_eq!(sources.len(), 2);
        assert_source(&receipt, first_request(&sources)?)?;
        let Some(ProducerDecisionStage::Attempted { source, .. }) = &receipt.decision_stage else {
            anyhow::bail!("actual completed U");
        };
        assert_eq!(source.event_id, sources[1].obj_id());
        for event in sources {
            assert!(
                h.network.repo.find_one(event.obj_id()).await? == Some(event.to_generic()),
                "actual signed server must store original R/U"
            );
        }
        let remote = PersistentObject::new(h.network.repo.clone());
        for vault in [
            h.p.find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
                .await?
                .context("actual local completed Vault")?,
            remote
                .find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
                .await?
                .context("actual remote completed Vault")?,
        ] {
            assert_eq!(vault.0.value.members().len(), 1);
            assert_eq!(vault.0.value.secrets.len(), 2);
            assert!(
                vault.0.value.secrets.contains(&h.primary)
                    && vault.0.value.secrets.contains(&h.secondary)
            );
        }
        Ok(Self { h, receipt })
    }
    async fn observe(
        &self,
        input: ProducerReference,
    ) -> Result<(Result<LiveboardProducerReceipt>, usize)> {
        let h = &self.h;
        let before = h.p.repo.inner.inner.get_db().await;
        let remote = h.network.repo.get_db().await;
        let writes = h.p.repo.inner.writes.load(Ordering::SeqCst);
        let material = h.p.repo.inner.material_writes.load(Ordering::SeqCst);
        let source_count = h.p.repo.sources().len();
        let network = h.network.counts();
        let outcome = h.service.reconcile_producer_receipt(input).await;
        assert!(
            before == h.p.repo.inner.inner.get_db().await,
            "reconcile must preserve actual local DB"
        );
        assert!(
            remote == h.network.repo.get_db().await,
            "read-only metadata must not create server business state"
        );
        assert!(
            terminal::context(h).await?.is_none(),
            "reconcile cannot resurrect completed context"
        );
        assert_eq!(h.p.repo.inner.writes.load(Ordering::SeqCst), writes);
        assert_eq!(
            h.p.repo.inner.material_writes.load(Ordering::SeqCst),
            material
        );
        assert_eq!(
            h.p.repo.sources().len(),
            source_count,
            "no new R/U appended or signed"
        );
        let after = h.network.counts();
        assert_eq!(after.0, network.0, "no business upload");
        assert_eq!(after.2, network.2, "no ordinary sync/GetState");
        assert!(
            after.1 <= network.1 + 1,
            "at most ONE real atomic observation"
        );
        Ok((outcome, after.1 - network.1))
    }
}
fn unknown(receipt: &LiveboardProducerReceipt) {
    assert_eq!(
        receipt.phase,
        ProducerPhase::Unknown,
        "RED: unsupported U/UUID cannot become Queued"
    );
    assert!(
        !["ready", "done", "settled"].contains(&receipt.readiness.as_str()),
        "RED: unsupported supplied metadata cannot become ready"
    );
    assert_eq!(receipt.error_code.as_deref(), Some("OperationUnavailable"));
    assert!(
        !matches!(
            &receipt.decision_stage,
            Some(ProducerDecisionStage::Attempted {
                observation: ProducerSourceObservation::RawAcknowledged,
                ..
            })
        ),
        "unsupported supplied U is not acknowledged"
    );
}
#[tokio::test]
async fn existing_actual_u_with_wrong_commitment_fails_before_query_after_context_clear()
-> Result<()> {
    let c = Completed::new().await?;
    let mut input = reference(&c.receipt);
    let actual = input.decision_stage.as_mut().context("actual supplied U")?;
    let mut bad = "0".repeat(64);
    if bad == actual.body_commitment {
        bad = "1".repeat(64);
    }
    actual.body_commitment = bad;
    let (outcome, queries) = c.observe(input).await?;
    assert_eq!(queries, 0);
    assert!(
        outcome.is_err(),
        "existing wrong U commitment must be rejected locally"
    );
    Ok(())
}
#[tokio::test]
async fn proven_absent_own_stream_u_cannot_inherit_completed_request_ack_or_ready() -> Result<()> {
    let c = Completed::new().await?;
    let mut input = reference(&c.receipt);
    let absent = input
        .decision_stage
        .as_mut()
        .context("actual supplied U template")?;
    absent.event_id = absent.event_id.clone().next();
    assert_eq!(absent.event_id.fqdn, input.request_stage.event_id.fqdn);
    assert!(
        c.h.p
            .repo
            .find_one(absent.event_id.clone())
            .await?
            .is_none()
    );
    assert!(
        c.h.network
            .repo
            .find_one(absent.event_id.clone())
            .await?
            .is_none()
    );
    // Valid metadata INPUT only: no fabricated DeviceLog body, signature or canonical result.
    ProducerReference::parse_json(&serde_json::to_string(&input)?)?;
    let (outcome, queries) = c.observe(input).await?;
    eprintln!(
        "SUPPLIED_U_ABSENT context_cleared=true actual_local_remote_absence=true queries={queries} result_ok={}",
        outcome.is_ok()
    );
    if let Ok(receipt) = outcome {
        unknown(&receipt);
    }
    Ok(())
}
#[tokio::test]
async fn actual_u_with_wrong_valid_uuid_after_context_clear_cannot_silently_rebind() -> Result<()> {
    let c = Completed::new().await?;
    let mut input = reference(&c.receipt);
    let known = input.request_id.clone().context("actual canonical UUID")?;
    let wrong = "00000000-0000-4000-8000-000000000002".to_string();
    assert_ne!(known, wrong);
    input.request_id = Some(wrong.clone());
    ProducerReference::parse_json(&serde_json::to_string(&input)?)?;
    let (outcome, queries) = c.observe(input).await?;
    eprintln!(
        "SUPPLIED_UUID_CONTRADICTION context_cleared=true actual_r_u=true queries={queries} result_ok={}",
        outcome.is_ok()
    );
    if let Ok(receipt) = outcome {
        unknown(&receipt);
        assert_eq!(
            receipt.request_id.as_ref(),
            Some(&wrong),
            "contradictory supplied UUID must not silently become actual UUID"
        );
    }
    Ok(())
}
