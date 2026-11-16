//! Actual applied local2→1 completion may clear only its existing exact context after reopen.
use super::{
    context_repo::{Fault, Repo},
    fixture::*,
};
use anyhow::{Context, Result};
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};
use meta_secret_core::node::{
    common::model::{liveboard_producer::*, meta_pass::PlainPassInfo},
    db::{
        descriptors::{
            local_removal_envelope::LocalRemovalEnvelopeDescriptor,
            vault_descriptor::VaultDescriptor,
        },
        events::{generic_log_event::*, local_remove_producer::LocalRemoveProducerContext},
        repo::generic_db::*,
    },
};
use std::sync::{Arc, atomic::Ordering};

pub(super) async fn context<R: KvLogEventRepo + Send + Sync>(
    h: &Harness<Repo<R>>,
) -> Result<Option<LocalRemoveProducerContext>> {
    Ok(h.p
        .find_tail_event(LocalRemovalEnvelopeDescriptor(h.actor.user_id()))
        .await?
        .and_then(|event| event.0.value.producer_context))
}
pub(super) struct Saved {
    pub state: ReopenState,
    pub receipt: LiveboardProducerReceipt,
    pub context: LocalRemoveProducerContext,
}
pub(super) async fn empty_vault<R: KvLogEventRepo + Send + Sync>(
    h: &Harness<Repo<R>>,
) -> Result<()> {
    for pass in [&h.primary, &h.secondary] {
        let receipt = h.delete(false, pass.clone()).await?;
        assert_eq!(receipt.canonical_state, ProducerCanonicalState::Applied);
    }
    let local =
        h.p.find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
            .await?
            .context("actual empty local Vault after checked Deletes")?;
    let remote = meta_secret_core::node::db::objects::persistent_object::PersistentObject::new(
        h.network.repo.clone(),
    );
    let canonical = remote
        .find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
        .await?
        .context("actual empty canonical Vault after checked Deletes")?;
    assert!(local.0.value.secrets.is_empty());
    assert!(canonical.0.value.secrets.is_empty());
    Ok(())
}
pub(super) async fn interrupt<R: KvLogEventRepo + Send + Sync>(
    backend: Arc<R>,
    fault: Fault,
) -> Result<Saved> {
    let inner = Arc::new(Repo::new(backend));
    let h = Harness::new(inner.clone(), 2).await?;
    let receipt = drive(&h, async {
        h.web.get_state().await?;
        empty_vault(&h).await?;
        inner.arm(fault);
        h.p.repo.reset_observation();
        let receipt = h.remove(false).await?;
        assert_eq!(
            inner.hits.load(Ordering::SeqCst),
            1,
            "must reach actual post-Applied context-save/clear boundary"
        );
        let sources = h.p.repo.sources();
        assert_eq!(sources.len(), 2);
        assert_source(&receipt, first_request(&sources)?)?;
        let Some(ProducerDecisionStage::Attempted { source, .. }) = &receipt.decision_stage else {
            anyhow::bail!("actual attempted U survived late failure");
        };
        assert_eq!(source.event_id, sources[1].obj_id());
        assert!(
            h.network
                .repo
                .find_one(sources[0].obj_id())
                .await?
                .is_some()
        );
        assert!(
            h.network
                .repo
                .find_one(sources[1].obj_id())
                .await?
                .is_some()
        );
        let vault =
            h.p.find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
                .await?
                .context("actual downloaded post-removal Vault")?;
        assert_eq!(vault.0.value.members().len(), 1);
        assert!(vault.0.value.is_member(h.actor.device_id()));
        assert!(!vault.0.value.is_member(h.target.device_id()));
        assert_eq!(receipt.canonical_state, ProducerCanonicalState::Applied);
        assert_eq!(receipt.material_stage, Some(ProducerMaterialStage::Ready));
        assert_eq!(receipt.phase, ProducerPhase::Unknown);
        let retained = context(&h)
            .await?
            .context("actual uncleared durable context")?;
        assert_eq!(retained.request_stage.source, receipt.request_stage.source);
        assert_eq!(
            retained.decision_stage.as_ref().map(|s| &s.source),
            Some(source)
        );
        assert_ne!(retained.admission_vault, vault.obj_id());
        Ok(receipt)
    })
    .await?;
    let retained = context(&h)
        .await?
        .context("durable context before closing backend")?;
    Ok(Saved {
        state: h.into_reopen(),
        receipt,
        context: retained,
    })
    // All actual local service/gateway/backend handles close here; only remote remains.
}

async fn reopen<R: KvLogEventRepo + Send + Sync>(
    backend: Arc<R>,
    saved: Saved,
    mobile: bool,
) -> Result<()> {
    let inner = Arc::new(Repo::new(backend));
    let h = Harness::reopen(inner.clone(), saved.state)?;
    drive(&h, async {
        assert!(context(&h).await?.as_ref() == Some(&saved.context));
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        inner.reset();
        let mut input = reference(&saved.receipt);
        input.decision_stage = None; // Real durable U is authoritative even when caller omits it.
        let before = context(&h).await?;
        let writes = inner.writes.load(Ordering::SeqCst);
        let network = h.network.counts();
        let observed = h.reconcile(mobile, input.clone()).await?;
        assert_eq!(
            observed.request_stage.source,
            saved.receipt.request_stage.source
        );
        assert_eq!(observed.canonical_state, ProducerCanonicalState::Applied);
        assert!(context(&h).await? == before);
        assert_eq!(inner.writes.load(Ordering::SeqCst), writes);
        assert_eq!(h.network.counts().0, network.0);
        let completed = h.resume(mobile, input).await?;
        assert!(
            context(&h).await?.is_none(),
            "RED: exact already-Applied local2→1 context remains permanently blocked after reopen"
        );
        assert_eq!(
            completed.request_stage.source,
            saved.receipt.request_stage.source
        );
        assert_eq!(completed.request_id, saved.receipt.request_id);
        assert_eq!(completed.canonical_state, ProducerCanonicalState::Applied);
        assert_eq!(completed.phase, ProducerPhase::Queued);
        assert_eq!(completed.material_stage, Some(ProducerMaterialStage::Ready));
        assert!(["ready", "done", "settled"].contains(&completed.readiness.as_str()));
        let original_u = saved
            .context
            .decision_stage
            .as_ref()
            .context("original actual U")?;
        assert!(matches!(&completed.decision_stage,
            Some(ProducerDecisionStage::Attempted { source, .. }) if *source == original_u.source));
        assert!(
            h.p.repo.sources().is_empty(),
            "completion-only cleanup cannot append/sign R2/U2"
        );
        assert_eq!(
            inner.material_writes.load(Ordering::SeqCst),
            0,
            "completion-only cleanup cannot regenerate material"
        );
        assert_eq!(h.network.counts().0, network.0, "no new business uploads");
        let fresh = PlainPassInfo::new(
            "after-terminal-cleanup".into(),
            "SYNTHETIC-AFTER-TERMINAL".into(),
        );
        let pass_id = fresh.pass_id.clone();
        h.web.cluster_distribution(fresh).await;
        let vault =
            h.p.find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
                .await?
                .context("actual created Secret after terminal context cleared")?;
        assert!(vault.0.value.secrets.contains(&pass_id));
        let next = h.delete(mobile, pass_id).await?;
        assert_eq!(
            next.canonical_state,
            ProducerCanonicalState::Applied,
            "cleared terminal context must permit next explicit checked operation"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn healthy_empty_vault_checked_local2to1_reaches_ready_and_clears_context_control()
-> Result<()> {
    let h = Harness::new(
        Arc::new(Repo::new(Arc::new(
            meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo::default(),
        ))),
        2,
    )
    .await?;
    drive(&h, async {
        h.web.get_state().await?;
        empty_vault(&h).await?;
        let receipt = h.remove(false).await?;
        assert_eq!(receipt.canonical_state, ProducerCanonicalState::Applied);
        assert_eq!(receipt.phase, ProducerPhase::Queued);
        assert_eq!(receipt.material_stage, Some(ProducerMaterialStage::Ready));
        assert!(["ready", "done", "settled"].contains(&receipt.readiness.as_str()));
        assert!(context(&h).await?.is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn sqlite_applied_clear_failure_reopen_completes_same_context_without_new_business()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!(
        "file:{}",
        dir.path().join("terminal-context.sqlite").display()
    );
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let saved = interrupt(
        Arc::new(SqlIteRepo {
            conn_url: url.clone(),
        }),
        Fault::ClearBefore,
    )
    .await?;
    reopen(Arc::new(SqlIteRepo { conn_url: url }), saved, false).await
}
#[tokio::test]
async fn redb_post_apply_context_save_failure_reopen_completes_same_u_without_new_material()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("terminal-context.redb");
    let saved = interrupt(
        Arc::new(ReDbRepo::new(&path)?),
        Fault::TerminalContextBefore,
    )
    .await?;
    reopen(Arc::new(ReDbRepo::open(&path)?), saved, true).await
}
