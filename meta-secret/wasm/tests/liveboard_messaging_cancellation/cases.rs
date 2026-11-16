use super::{fixtures::VALUE, repo::Harness};
use anyhow::{Context, Result};
use meta_secret_core::node::common::model::{
    ApplicationState, liveboard::LiveboardOperationReceipt, meta_pass::PlainPassInfo,
};
use std::{future::Future, time::Duration};

async fn drive<T>(h: &Harness, scenario: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::select! {
        result = scenario => result,
        result = h.service.run() => { result?; anyhow::bail!("service ended before cancellation scenario") },
        _ = tokio::time::sleep(Duration::from_secs(10)) => anyhow::bail!("cancellation scenario watchdog elapsed"),
    }
}
fn input(name: &str) -> PlainPassInfo {
    PlainPassInfo::new(name.into(), VALUE.into())
}

async fn checked(h: &Harness, mobile: bool, name: &str) -> Result<LiveboardOperationReceipt> {
    if mobile {
        h.mobile.cluster_distribution_checked(input(name)).await
    } else {
        h.web.cluster_distribution_checked(input(name)).await
    }
}
async fn assert_own_id(
    h: &Harness,
    receipt: &LiveboardOperationReceipt,
    name: &str,
    count: usize,
) -> Result<()> {
    let added = h.added().await;
    assert_eq!(
        added.len(),
        count,
        "cancellation must not resubmit a command"
    );
    let actual = added
        .get(name)
        .context("surviving caller must persist its own operation")?;
    assert_eq!(
        receipt.secret_id.as_ref(),
        Some(actual),
        "surviving caller received another operation's ID"
    );
    Ok(())
}

async fn checked_to_checked(mobile: bool) -> Result<()> {
    let h = Harness::new().await?;
    h.p.repo.barrier.arm_save();
    let mut canceled = Box::pin(checked(&h, mobile, "canceled-create"));
    assert!(
        futures_lite::future::poll_once(canceled.as_mut())
            .await
            .is_none()
    );
    let scenario = async {
        h.p.repo.barrier.wait().await;
        h.assert_boundary().await?;
        assert_eq!(
            h.added().await.len(),
            1,
            "canceled operation reached real persistence"
        );
        drop(canceled);
        let survivor = checked(&h, mobile, "surviving-create");
        h.p.repo.barrier.release();
        let result = survivor.await;
        h.wait_creates(2).await;
        let receipt = result.context("surviving checked caller must get its own response")?;
        assert_own_id(&h, &receipt, "surviving-create", 2).await
    };
    drive(&h, scenario).await
}

#[tokio::test]
async fn canceled_web_checked_reply_cannot_become_next_checked_operations_identity() -> Result<()> {
    checked_to_checked(false).await
}
#[tokio::test]
async fn canceled_mobile_checked_reply_cannot_become_next_checked_operations_identity() -> Result<()>
{
    checked_to_checked(true).await
}

#[tokio::test]
async fn canceled_checked_reply_cannot_poison_legacy_get_state_response_type() -> Result<()> {
    let h = Harness::new().await?;
    h.p.repo.barrier.arm_save();
    let mut canceled = Box::pin(
        h.web
            .cluster_distribution_checked(input("canceled-checked")),
    );
    assert!(
        futures_lite::future::poll_once(canceled.as_mut())
            .await
            .is_none()
    );
    let scenario = async {
        h.p.repo.barrier.wait().await;
        h.assert_boundary().await?;
        drop(canceled);
        let survivor = h.web.get_state();
        h.p.repo.barrier.release();
        let state = survivor
            .await
            .context("GetState must not consume abandoned operation receipt")?;
        assert!(matches!(state, ApplicationState::Vault(_)));
        assert_eq!(
            h.added().await.len(),
            1,
            "GetState cannot duplicate a canceled Create"
        );
        Ok(())
    };
    drive(&h, scenario).await
}

#[tokio::test]
async fn canceled_legacy_get_state_reply_cannot_poison_checked_operation_result() -> Result<()> {
    let h = Harness::new().await?;
    let scenario = async {
        let baseline = h.web.get_state().await?;
        assert!(
            matches!(baseline, ApplicationState::Vault(_)),
            "service startup must finish before arming legacy read"
        );
        h.p.repo.barrier.arm_read();
        let mut canceled = Box::pin(h.web.get_state());
        assert!(
            futures_lite::future::poll_once(canceled.as_mut())
                .await
                .is_none()
        );
        h.p.repo.barrier.wait().await;
        h.assert_boundary().await?;
        drop(canceled);
        let survivor = h
            .web
            .cluster_distribution_checked(input("after-canceled-state"));
        h.p.repo.barrier.release();
        let result = survivor.await;
        h.wait_creates(1).await;
        let receipt =
            result.context("checked operation must not consume abandoned ApplicationState")?;
        assert_own_id(&h, &receipt, "after-canceled-state", 1).await
    };
    drive(&h, scenario).await
}

#[tokio::test]
async fn uncanceled_checked_callers_keep_their_actual_saved_identities_through_same_barrier()
-> Result<()> {
    let h = Harness::new().await?;
    h.p.repo.barrier.arm_save();
    let mut first = Box::pin(h.web.cluster_distribution_checked(input("first-control")));
    assert!(
        futures_lite::future::poll_once(first.as_mut())
            .await
            .is_none()
    );
    let scenario = async {
        h.p.repo.barrier.wait().await;
        h.assert_boundary().await?;
        h.p.repo.barrier.release();
        let first_receipt = first.await?;
        assert_own_id(&h, &first_receipt, "first-control", 1).await?;
        let second = h
            .web
            .cluster_distribution_checked(input("second-control"))
            .await?;
        assert_own_id(&h, &second, "second-control", 2).await?;
        assert_ne!(first_receipt.secret_id, second.secret_id);
        Ok(())
    };
    drive(&h, scenario).await
}

#[tokio::test]
async fn dropping_unpolled_call_before_enqueue_does_not_poison_the_next_receipt() -> Result<()> {
    let h = Harness::new().await?;
    drop(Box::pin(
        h.mobile
            .cluster_distribution_checked(input("never-enqueued")),
    ));
    assert!(h.added().await.is_empty());
    let scenario = async {
        let receipt = h
            .mobile
            .cluster_distribution_checked(input("only-persisted"))
            .await?;
        assert_own_id(&h, &receipt, "only-persisted", 1).await?;
        assert!(!h.added().await.contains_key("never-enqueued"));
        Ok(())
    };
    drive(&h, scenario).await
}
