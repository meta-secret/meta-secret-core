//! Actual manager/service lifecycle boundaries, without production changes.
#![cfg(not(target_arch = "wasm32"))]
#[path = "../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "liveboard_messaging_cancellation/repo.rs"]
#[allow(dead_code)]
mod repo;
#[path = "../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;

use anyhow::{Context, Result};
use meta_secret_core::node::{
    app::meta_app::{
        messaging::GenericAppStateRequest as Q, meta_client_service::MetaClientService,
    },
    common::model::{ApplicationState, meta_pass::PlainPassInfo},
};
use repo::Harness;
use std::{
    future::Future,
    pin::{Pin, pin},
    time::Duration,
};

fn input(name: &str) -> PlainPassInfo {
    PlainPassInfo::new(name.into(), fixtures::VALUE.into())
}
async fn poll_once<F: Future>(mut future: std::pin::Pin<&mut F>) -> Option<F::Output> {
    std::future::poll_fn(|cx| {
        std::task::Poll::Ready(match future.as_mut().poll(cx) {
            std::task::Poll::Ready(result) => Some(result),
            std::task::Poll::Pending => None,
        })
    })
    .await
}
async fn drive<T>(h: &Harness, scenario: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::select! {
        result = scenario => result,
        result = h.service.run() => { result?; anyhow::bail!("owner ended before real scenario completed") },
        _ = tokio::time::sleep(Duration::from_secs(10)) => anyhow::bail!("service edge watchdog elapsed"),
    }
}
async fn reach_barrier<F: Future<Output = Result<()>>>(
    h: &Harness,
    owner: &mut Pin<Box<F>>,
) -> Result<()> {
    tokio::select! {
        _ = h.p.repo.barrier.wait() => h.assert_boundary().await,
        _ = owner.as_mut() => anyhow::bail!("owner exited before the real repository barrier"),
        _ = tokio::time::sleep(Duration::from_secs(10)) => anyhow::bail!("repository boundary watchdog elapsed"),
    }
}

#[tokio::test]
async fn observed_full_queue_canceled_unaccepted_checked_sender_causes_zero_extra_writes()
-> Result<()> {
    let h = Harness::new().await?;
    for index in 0..10 {
        h.service
            .data_transfer
            .dt
            .send_to_service(Q::ClusterDistribution(input(&format!("full-{index}"))))
            .await;
    }
    let mut capacity_probe = Box::pin(h.service.data_transfer.dt.send_to_service(Q::GetState));
    assert!(
        poll_once(capacity_probe.as_mut()).await.is_none(),
        "raw no-reply send must observe actual bounded capacity, not a response wait"
    );
    drop(capacity_probe);
    let mut never_accepted = Box::pin(h.web.cluster_distribution_checked(input("never-accepted")));
    assert!(poll_once(never_accepted.as_mut()).await.is_none());
    drop(never_accepted);
    let scenario = async {
        h.wait_creates(10).await;
        let added = h.added().await;
        assert_eq!(added.len(), 10);
        assert!(!added.contains_key("never-accepted"));
        for index in 0..10 {
            assert!(added.contains_key(&format!("full-{index}")));
        }
        Ok(())
    };
    drive(&h, scenario).await
}

#[tokio::test]
async fn twelve_real_fire_and_forget_creates_do_not_stall_following_checked_receipt() -> Result<()>
{
    let h = Harness::new().await?;
    let scenario = async {
        for index in 0..12 {
            h.service
                .data_transfer
                .dt
                .send_to_service(Q::ClusterDistribution(input(&format!("no-reply-{index}"))))
                .await;
        }
        let receipt = h
            .web
            .cluster_distribution_checked(input("after-twelve"))
            .await
            .context("correlated call must not consume a fire-and-forget AppState")?;
        h.wait_creates(13).await;
        let added = h.added().await;
        assert_eq!(added.len(), 13, "each accepted command persists once");
        assert_eq!(receipt.secret_id.as_ref(), added.get("after-twelve"));
        Ok(())
    };
    drive(&h, scenario).await
}

#[tokio::test]
async fn dropping_run_during_confirmed_startup_read_releases_prequeued_caller_without_write()
-> Result<()> {
    let h = Harness::new().await?;
    let mut caller = pin!(h.web.cluster_distribution_checked(input("not-executed")));
    assert!(poll_once(caller.as_mut()).await.is_none());
    h.p.repo.barrier.arm_read();
    let mut owner = Box::pin(h.service.run());
    reach_barrier(&h, &mut owner).await?;
    drop(owner);
    assert!(
        matches!(poll_once(caller.as_mut()).await, Some(Err(_))),
        "startup owner drop must disconnect its original queued caller immediately"
    );
    assert!(h.added().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn dropping_run_after_actual_save_releases_active_and_queued_callers_without_resubmit()
-> Result<()> {
    let h = Harness::new().await?;
    let mut active = pin!(h.web.cluster_distribution_checked(input("uncertain-saved")));
    let mut queued = pin!(h.web.get_state());
    let mut next = pin!(
        h.mobile
            .cluster_distribution_checked(input("queued-never-run"))
    );
    assert!(poll_once(active.as_mut()).await.is_none());
    assert!(poll_once(queued.as_mut()).await.is_none());
    assert!(poll_once(next.as_mut()).await.is_none());
    h.p.repo.barrier.arm_save();
    let mut owner = Box::pin(h.service.run());
    reach_barrier(&h, &mut owner).await?;
    assert_eq!(h.added().await.len(), 1);
    drop(owner);
    assert!(
        matches!(poll_once(active.as_mut()).await, Some(Err(_))),
        "active reply must drop with the owner"
    );
    assert!(
        matches!(poll_once(queued.as_mut()).await, Some(Err(_))),
        "queued legacy caller must disconnect"
    );
    assert!(
        matches!(poll_once(next.as_mut()).await, Some(Err(_))),
        "queued checked caller must disconnect"
    );
    let added = h.added().await;
    assert_eq!(added.len(), 1);
    assert!(added.contains_key("uncertain-saved") && !added.contains_key("queued-never-run"));
    Ok(())
}

#[tokio::test]
async fn rejected_second_run_during_startup_does_not_steal_first_service_ownership() -> Result<()> {
    let h = Harness::new().await?;
    h.p.repo.barrier.arm_read();
    let mut first = Box::pin(h.service.run());
    reach_barrier(&h, &mut first).await?;
    let mut second = pin!(h.service.run());
    assert!(
        matches!(poll_once(second.as_mut()).await, Some(Err(_))),
        "second run must reject rather than share the receive queue"
    );
    h.p.repo.barrier.release();
    let caller = async {
        let receipt = h
            .web
            .cluster_distribution_checked(input("first-owner-still-live"))
            .await?;
        let added = h.added().await;
        assert_eq!(added.len(), 1);
        assert_eq!(
            receipt.secret_id.as_ref(),
            added.get("first-owner-still-live")
        );
        anyhow::Ok(())
    };
    tokio::select! {
        result = caller => result,
        result = first.as_mut() => { result?; anyhow::bail!("first owner exited after rejected second claim") },
        _ = tokio::time::sleep(Duration::from_secs(10)) => anyhow::bail!("first owner control watchdog elapsed"),
    }
}

#[tokio::test]
async fn actual_startup_decryption_failure_closes_prequeued_callers_without_executing_them()
-> Result<()> {
    use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
    let h = Harness::new().await?;
    let mut checked = pin!(
        h.web
            .cluster_distribution_checked(input("queued-before-startup-error"))
    );
    let mut legacy = pin!(h.web.get_state());
    assert!(poll_once(checked.as_mut()).await.is_none());
    assert!(poll_once(legacy.as_mut()).await.is_none());
    let wrong_master = FixtureRegistry::empty()
        .state
        .device_creds
        .client_master_key;
    let failing_owner = MetaClientService {
        data_transfer: h.service.data_transfer.clone(),
        sync_gateway: h.service.sync_gateway.clone(),
        state_provider: h.service.state_provider.clone(),
        p_obj: h.p.clone(),
        device_data: h.service.device_data.clone(),
        master_key: wrong_master,
    };
    assert!(
        failing_owner.run().await.is_err(),
        "actual credential decryption must reject the startup key"
    );
    assert!(
        matches!(poll_once(checked.as_mut()).await, Some(Err(_))),
        "startup failure must disconnect queued checked reply"
    );
    assert!(
        matches!(poll_once(legacy.as_mut()).await, Some(Err(_))),
        "startup failure must disconnect queued legacy reply"
    );
    assert!(h.added().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn simultaneous_real_checked_legacy_checked_callers_keep_each_saved_id_and_state_type()
-> Result<()> {
    let h = Harness::new().await?;
    let scenario = async {
        let (first, state, second) = tokio::join!(
            h.web.cluster_distribution_checked(input("live-first")),
            h.web.get_state(),
            h.mobile.cluster_distribution_checked(input("live-second")),
        );
        let first = first?;
        let second = second?;
        assert!(matches!(state?, ApplicationState::Vault(_)));
        let added = h.added().await;
        assert_eq!(added.len(), 2);
        assert_eq!(first.secret_id.as_ref(), added.get("live-first"));
        assert_eq!(second.secret_id.as_ref(), added.get("live-second"));
        assert_ne!(first.secret_id, second.secret_id);
        Ok(())
    };
    drive(&h, scenario).await
}
