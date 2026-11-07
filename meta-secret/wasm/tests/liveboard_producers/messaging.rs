use super::fixture::*;
use anyhow::{Context, Result};
use meta_secret_core::node::{
    common::model::{ApplicationState, meta_pass::PlainPassInfo},
    db::{events::generic_log_event::*, in_mem_db::InMemKvLogEventRepo, repo::generic_db::*},
};
use std::sync::Arc;

async fn enqueues(mobile: bool, remove: bool) -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    let mut operation = Box::pin(async {
        if remove {
            h.remove(mobile).await
        } else {
            h.delete(mobile, h.primary.clone()).await
        }
    });
    let baseline = h.p.repo.writes.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        futures_lite::future::poll_once(operation.as_mut())
            .await
            .is_none(),
        "RED: checked producer must enqueue rather than finish in a separate manager lane"
    );
    let calls = h
        .service
        .data_transfer
        .dt
        .service_drain()?
        .collect::<Vec<_>>();
    assert_eq!(
        calls.len(),
        1,
        "exactly one actual same-FIFO producer envelope"
    );
    assert_eq!(
        calls[0].kind(),
        if remove {
            "RemoveMemberChecked"
        } else {
            "DeleteSecretChecked"
        }
    );
    assert_eq!(
        h.p.repo.writes.load(std::sync::atomic::Ordering::SeqCst),
        baseline,
        "no effect before owner receives envelope"
    );
    for call in calls {
        h.service.data_transfer.dt.requeue_call(call)?;
    }
    drive(&h, async {
        operation.await.context("actual queued producer result")?;
        Ok(())
    })
    .await
}
#[tokio::test]
async fn web_delete_uses_exact_one_existing_service_fifo_before_effects() -> Result<()> {
    enqueues(false, false).await
}
#[tokio::test]
async fn mobile_remove_uses_exact_one_existing_service_fifo_before_effects() -> Result<()> {
    enqueues(true, true).await
}

async fn canceled_checked(mobile: bool) -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        h.p.repo.barrier.arm_request();
        let mut canceled = Box::pin(h.delete(mobile, h.primary.clone()));
        assert!(
            futures_lite::future::poll_once(canceled.as_mut())
                .await
                .is_none(),
            "RED: actual producer enqueue required"
        );
        h.p.repo.barrier.wait().await;
        let id = h.p.repo.barrier.id()?;
        assert!(
            h.p.repo.inner.find_one(id.clone()).await?.is_some(),
            "cancel only after actual persisted R"
        );
        drop(canceled);
        let survivor = h.delete(mobile, h.secondary.clone());
        h.p.repo.barrier.release();
        let receipt = survivor
            .await
            .context("survivor must receive its own real source/UUID")?;
        let sources = h.p.repo.sources();
        assert_eq!(
            sources.len(),
            2,
            "cancellation must never resubmit or duplicate producer R"
        );
        assert_ne!(sources[0].obj_id(), sources[1].obj_id());
        assert_source(&receipt, &sources[1])?;
        assert_eq!(
            receipt.secret_id.as_deref(),
            Some(h.secondary.id_str().as_str())
        );
        Ok(())
    })
    .await
}
#[tokio::test]
async fn canceled_web_delete_cannot_poison_next_producer_identity() -> Result<()> {
    canceled_checked(false).await
}
#[tokio::test]
async fn canceled_mobile_delete_cannot_poison_next_producer_identity() -> Result<()> {
    canceled_checked(true).await
}

#[tokio::test]
async fn canceled_producer_reply_cannot_become_legacy_state_or_old_checked_receipt() -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        h.p.repo.barrier.arm_request();
        let mut canceled = Box::pin(h.delete(false, h.primary.clone()));
        assert!(
            futures_lite::future::poll_once(canceled.as_mut())
                .await
                .is_none(),
            "RED: actual producer enqueue required"
        );
        h.p.repo.barrier.wait().await;
        let id = h.p.repo.barrier.id()?;
        assert!(h.p.repo.inner.find_one(id).await?.is_some());
        drop(canceled);
        h.p.repo.barrier.release();
        assert!(matches!(
            h.mobile.get_state().await?,
            ApplicationState::Vault(_)
        ));
        let receipt = h
            .web
            .cluster_distribution_checked(PlainPassInfo::new(
                "mixed-old-receipt".into(),
                "SYNTHETIC-PRIVATE-MIXED-VALUE".into(),
            ))
            .await?;
        assert_eq!(serde_json::to_value(&receipt)?["kind"], "create");
        assert!(receipt.secret_id.is_some());
        assert_eq!(
            h.p.repo.sources().len(),
            1,
            "legacy state/old checked response cannot reissue abandoned Delete"
        );
        Ok(())
    })
    .await
}
#[tokio::test]
async fn canceled_legacy_get_state_cannot_poison_new_producer_receipt() -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        h.p.repo.barrier.arm_read();
        let mut canceled = Box::pin(h.web.get_state());
        assert!(
            futures_lite::future::poll_once(canceled.as_mut())
                .await
                .is_none()
        );
        h.p.repo.barrier.wait().await;
        let id = h.p.repo.barrier.id()?;
        assert!(h.p.repo.inner.find_one(id).await?.is_some());
        drop(canceled);
        h.p.repo.barrier.release();
        let receipt = h
            .delete(true, h.primary.clone())
            .await
            .context("RED: typed producer receipt cannot consume abandoned state")?;
        let sources = h.p.repo.sources();
        assert_eq!(sources.len(), 1);
        assert_source(&receipt, &sources[0])?;
        Ok(())
    })
    .await
}
