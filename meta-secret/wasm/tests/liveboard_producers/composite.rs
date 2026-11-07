//! Composite local2→1 keeps actual independent R/U sources across uncertainty.
use super::{fixture::*, repo::Fault};
use anyhow::{Context, Result};
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};
use meta_secret_core::node::{
    common::model::liveboard_producer::*,
    db::{
        events::{
            generic_log_event::*,
            vault::{device_log_event::DeviceLogObject, vault_log_event::*},
        },
        in_mem_db::InMemKvLogEventRepo,
        repo::generic_db::*,
    },
};
use std::sync::{Arc, atomic::Ordering};

fn decision(sources: &[DeviceLogObject]) -> Result<&DeviceLogObject> {
    sources
        .iter()
        .find(|source| {
            matches!(
                source.0.value,
                VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(_))
            )
        })
        .context("actual attempted existing local2→1 U")
}
fn assert_decision(
    receipt: &LiveboardProducerReceipt,
    actual: &DeviceLogObject,
) -> Result<ProducerSourceRef> {
    let Some(ProducerDecisionStage::Attempted { source, .. }) = &receipt.decision_stage else {
        anyhow::bail!("attempted U must never become notAttempted")
    };
    assert_eq!(source.event_id, actual.obj_id());
    let body = String::from_utf8(meta_secret_core::node::security::canonical_json(actual)?)?;
    assert!(
        source.body_commitment
            == meta_secret_core::crypto::utils::sha256_hex(&format!(
                "MetaSecret.LiveboardProducer.Source.v1\n{body}"
            )),
        "actual exact U commitment"
    );
    Ok(source.clone())
}
async fn boundary(fault: Fault) -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 2).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        h.p.repo.arm(fault);
        let receipt = h
            .remove(false)
            .await
            .context("RED: composite producer must preserve actual R/U stages")?;
        assert_eq!(h.p.repo.fault_hits.load(Ordering::SeqCst), 1);
        let sources = h.p.repo.sources();
        let r = first_request(&sources)?;
        assert_source(&receipt, r)?;
        if sources.len() == 1 {
            assert_eq!(receipt.phase, ProducerPhase::Unknown);
            assert_eq!(
                receipt.decision_stage,
                Some(ProducerDecisionStage::NotAttempted)
            );
            assert_eq!(
                receipt.material_stage,
                Some(ProducerMaterialStage::NotStarted)
            );
            assert!(h.p.repo.inner.find_one(r.obj_id()).await?.is_none());
        } else {
            let u = decision(&sources)?;
            assert_decision(&receipt, u)?;
            assert!(h.p.repo.inner.find_one(r.obj_id()).await?.is_some());
            if h.p.repo.inner.find_one(u.obj_id()).await?.is_none() {
                assert_eq!(receipt.phase, ProducerPhase::Unknown);
                assert_eq!(
                    receipt.material_stage,
                    Some(ProducerMaterialStage::NotStarted)
                );
            }
        }
        Ok(())
    })
    .await
}
#[tokio::test]
async fn local_remove_r_failed_before_body_reports_not_attempted_u_and_no_material() -> Result<()> {
    boundary(Fault::RequestBefore).await
}
#[tokio::test]
async fn local_remove_u_failed_before_body_retains_actual_u_identity() -> Result<()> {
    boundary(Fault::DecisionBefore).await
}
#[tokio::test]
async fn local_remove_u_lost_reply_never_forgets_actual_u_identity() -> Result<()> {
    boundary(Fault::DecisionLostReply).await
}

struct Interrupted {
    state: ReopenState,
    receipt: LiveboardProducerReceipt,
    actual_u: DeviceLogObject,
}
async fn interrupt<R: KvLogEventRepo + Send + Sync>(inner: Arc<R>) -> Result<Interrupted> {
    let h = Harness::new(inner, 2).await?;
    let receipt = drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        h.p.repo.arm(Fault::DecisionBefore);
        h.remove(false)
            .await
            .context("RED: actual U reservation receipt before file reopen")
    })
    .await?;
    assert_eq!(h.p.repo.fault_hits.load(Ordering::SeqCst), 1);
    let sources = h.p.repo.sources();
    let u = decision(&sources)?.clone();
    assert_decision(&receipt, &u)?;
    assert!(h.p.repo.inner.find_one(u.obj_id()).await?.is_none());
    Ok(Interrupted {
        state: h.into_reopen(),
        receipt,
        actual_u: u,
    })
    // service/gateway/local backend handles close; only remote server remains alive.
}
async fn reopened<R: KvLogEventRepo + Send + Sync>(
    inner: Arc<R>,
    saved: Interrupted,
    mobile: bool,
) -> Result<()> {
    let expected_r = saved.receipt.request_stage.source.clone();
    let expected_u = assert_decision(&saved.receipt, &saved.actual_u)?;
    let h = Harness::reopen(inner, saved.state)?;
    drive(&h, async {
        // Startup is an ordinary boundary, then all subsequent reconcile effects
        // are measured independently. Omitted U cannot erase durable Core context.
        assert!(
            h.p.repo
                .inner
                .find_one(saved.actual_u.obj_id())
                .await?
                .is_none()
        );
        h.web.get_state().await?;
        assert!(
            h.p.repo
                .inner
                .find_one(saved.actual_u.obj_id())
                .await?
                .is_none(),
            "ordinary startup cannot create U without fresh explicit confirmation"
        );
        h.p.repo.reset_observation();
        let mut input = reference(&saved.receipt);
        input.decision_stage = None;
        let writes = h.p.repo.writes.load(Ordering::SeqCst);
        let network = h.network.counts();
        let observed = h
            .reconcile(mobile, input.clone())
            .await
            .context("same R/U must survive actual local DB reopen")?;
        assert_eq!(observed.request_stage.source, expected_r);
        let Some(ProducerDecisionStage::Attempted { source, .. }) = &observed.decision_stage else {
            anyhow::bail!("durable attempted U lost when client omitted U input")
        };
        assert_eq!(source, &expected_u);
        assert_eq!(h.p.repo.writes.load(Ordering::SeqCst), writes);
        assert_eq!(h.network.counts().0, network.0);
        assert!(
            h.p.repo.sources().is_empty(),
            "read-only reconciliation may not resume U"
        );
        let completed = h
            .resume(mobile, input)
            .await
            .context("fresh explicit existing local confirmation resumes same frozen U")?;
        assert_eq!(completed.request_stage.source, expected_r);
        assert_decision(&completed, &saved.actual_u)?;
        for attempted in h.p.repo.sources() {
            assert!(
                attempted == saved.actual_u,
                "continuation must not allocate R2/U2 or change deterministic U"
            );
        }
        assert!(
            h.p.repo.inner.find_one(saved.actual_u.obj_id()).await?
                == Some(saved.actual_u.clone().to_generic())
        );
        Ok(())
    })
    .await
}
#[tokio::test]
async fn sqlite_local2to1_reopen_reconcile_is_read_only_and_explicit_resume_uses_same_u()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!("file:{}", dir.path().join("producer.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let saved = interrupt(Arc::new(SqlIteRepo {
        conn_url: url.clone(),
    }))
    .await?;
    reopened(Arc::new(SqlIteRepo { conn_url: url }), saved, false).await
}
#[tokio::test]
async fn redb_local2to1_reopen_reconcile_uses_durable_u_when_native_reference_omits_it()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("producer.redb");
    let saved = interrupt(Arc::new(ReDbRepo::new(&path)?)).await?;
    reopened(Arc::new(ReDbRepo::open(&path)?), saved, true).await
}
#[tokio::test]
async fn legacy_local2to1_actual_r_and_u_with_real_crypto_is_positive_fixture_control() -> Result<()>
{
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 2).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        h.service
            .remove_member(h.target.device_id().to_string())
            .await?;
        let sources = h.p.repo.sources();
        assert_eq!(sources.len(), 2);
        let r = first_request(&sources)?;
        let u = decision(&sources)?;
        assert_ne!(r.obj_id(), u.obj_id());
        assert!(
            h.p.repo.material_attempts.load(Ordering::SeqCst) > 0,
            "actual local material stage must be observable by the fault fixture"
        );
        assert!(h.network.repo.find_one(r.obj_id()).await?.is_some());
        assert!(h.network.repo.find_one(u.obj_id()).await?.is_some());
        Ok(())
    })
    .await
}

async fn material_failure(fault: Fault) -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 2).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        h.p.repo.arm(fault);
        let receipt = h
            .remove(true)
            .await
            .context("RED: composite producer must preserve R/U when actual material work fails")?;
        assert_eq!(h.p.repo.fault_hits.load(Ordering::SeqCst), 1);
        assert!(h.p.repo.material_attempts.load(Ordering::SeqCst) > 0);
        let sources = h.p.repo.sources();
        let r = first_request(&sources)?;
        let u = decision(&sources)?;
        assert_source(&receipt, r)?;
        assert_decision(&receipt, u)?;
        assert!(h.p.repo.inner.find_one(r.obj_id()).await?.is_some());
        assert!(h.p.repo.inner.find_one(u.obj_id()).await?.is_some());
        assert_eq!(receipt.phase, ProducerPhase::Unknown);
        assert!(matches!(
            receipt.material_stage,
            Some(ProducerMaterialStage::Pending | ProducerMaterialStage::Unknown)
        ));
        assert!(!["ready", "done", "settled"].contains(&receipt.readiness.as_str()));
        let writes = h.p.repo.writes.load(Ordering::SeqCst);
        let input = reference(&receipt);
        let observed = h.reconcile(true, input).await?;
        assert_eq!(observed.request_stage.source, receipt.request_stage.source);
        assert_eq!(observed.decision_stage, receipt.decision_stage);
        assert_eq!(
            h.p.repo.writes.load(Ordering::SeqCst),
            writes,
            "read-only reconcile must not finalize incomplete material"
        );
        Ok(())
    })
    .await
}
#[tokio::test]
async fn local2to1_material_failure_cannot_mark_r_u_composite_queued_or_ready() -> Result<()> {
    material_failure(Fault::MaterialBefore).await
}
#[tokio::test]
async fn local2to1_partial_material_lost_reply_keeps_exact_r_u_and_read_only_reconcile()
-> Result<()> {
    material_failure(Fault::MaterialLostReply).await
}
