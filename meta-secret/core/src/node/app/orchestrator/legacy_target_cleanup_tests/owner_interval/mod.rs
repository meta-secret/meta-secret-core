//! Public legacy methods share the actual sync owner; no new-generation loss is simulated.
mod barrier;

use super::*;
use crate::crypto::key_pair::{KeyPair, TransportDsaKeyPair};
use crate::node::api::{DataSyncResponse, ReadSyncRequest, SyncRequest};
use crate::node::app::sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol};
use crate::node::common::model::user::common::UserData;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::task::Poll;

#[derive(Clone, Copy, Debug)]
enum Interval {
    Accept,
    Finalize,
}

async fn once<F: Future>(mut future: Pin<&mut F>) -> Poll<F::Output> {
    poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx))).await
}
fn boundary(result: &Result<()>) -> bool {
    result
        .as_ref()
        .err()
        .is_some_and(|error| error.is::<barrier::TransportBoundary>())
}

#[tokio::test]
async fn public_accept_and_finalize_hold_same_sync_owner_before_reads_through_target_delete()
-> Result<()> {
    let mut results = vec![];
    for interval in [Interval::Accept, Interval::Finalize] {
        results.push(run(interval).await);
    }
    for result in results {
        result?;
    }
    Ok(())
}

async fn run(interval: Interval) -> Result<()> {
    let h = fixture::Harness::new().await?;
    if matches!(interval, Interval::Finalize) {
        h.world.repo.arm(h.old_target.obj_id());
        let failed = h.approve().await;
        ensure!(
            failed
                .as_ref()
                .err()
                .is_some_and(|e| e.is::<repo::TargetDeleteBefore>())
                && h.world.repo.fault_hits.load(Ordering::SeqCst) == 1,
            "finalizer setup must reach actual target delete BeforeErr"
        );
        proof::publication(&h).await?;
        let retained = h.apply_actual_update().await?;
        ensure!(
            !retained,
            "actual canonical completion must keep updates empty"
        );
        h.world.repo.clear();
    }
    let before = proof::snapshot(&h).await;
    let (delete_release, delete_wait) = flume::unbounded();
    let (sync_release, sync_wait) = flume::unbounded();
    let outer = Arc::new(barrier::Outer::new(&h, delete_wait));
    let original = &h.world.a.p_obj;
    let p = Arc::new(PersistentObject {
        repo: outer.clone(),
        device_log_append_guard: original.device_log_append_guard.clone(),
        client_operation_guard: original.client_operation_guard.clone(),
        device_sync_guard: original.device_sync_guard.clone(),
    });
    ensure!(
        Arc::ptr_eq(
            &p.device_log_append_guard,
            &original.device_log_append_guard
        ) && Arc::ptr_eq(&p.client_operation_guard, &original.client_operation_guard)
            && Arc::ptr_eq(&p.device_sync_guard, &original.device_sync_guard),
        "new observer changed SAME three ownership Arcs"
    );
    let actor = MetaOrchestrator {
        p_obj: p.clone(),
        user_creds: h.world.a.user_creds.clone(),
    };
    let master = TransportDsaKeyPair::generate().sk();
    let credentials = PersistentCredentials {
        p_obj: p.clone(),
        master_key: master.clone(),
    };
    credentials
        .save_device_creds(actor.user_creds.device_creds.clone())
        .await?;
    credentials
        .save_user_creds(actor.user_creds.clone())
        .await?;
    ensure!(
        credentials
            .get_user_creds()
            .await?
            .is_some_and(|actual| actual.user() == actor.user_creds.user()),
        "actual stored fixture identity changed"
    );
    let wire = Arc::new(barrier::Wire::new(actor.user_creds.user(), sync_wait));
    let gateway = SyncGateway::new(
        "real-legacy-owner-component".into(),
        p.clone(),
        wire.clone(),
        master,
    );
    ensure!(
        Arc::ptr_eq(&actor.p_obj, &gateway.p_obj),
        "legacy API and sync do not share ONE actual object"
    );
    h.world.repo.reset();

    let mut first_sync = Box::pin(gateway.sync(actor.user_creds.user()));
    let first_poll = once(first_sync.as_mut()).await;
    ensure!(
        first_poll.is_pending() && wire.first_entered.load(Ordering::SeqCst),
        "actual first public sync transport barrier UNREACHED"
    );
    let cursor = outer.calls();
    let mut legacy = Box::pin(async {
        match interval {
            Interval::Accept => {
                actor
                    .accept_remove_member(h.world.c.user_creds.device_id().to_string())
                    .await
            }
            Interval::Finalize => actor.finalize_pending_removal_resharing().await,
        }
    });
    let first_api_poll = once(legacy.as_mut()).await;
    let entry_pending = first_api_poll.is_pending();
    let entry_calls = outer.calls() - cursor;
    let entry_old_unchanged = h.world.repo.inner.find_one(h.old_target.obj_id()).await?
        == Some(h.old_target.clone().to_generic());
    // Always release the actual owner before evaluating missing-ownership RED.
    sync_release.send(()).context("release first public sync")?;
    let first_result = first_sync.await;
    let mut early = match first_api_poll {
        Poll::Ready(result) => Some(result),
        Poll::Pending => None,
    };
    if early.is_none()
        && !outer.delete_entered.load(Ordering::SeqCst)
        && let Poll::Ready(result) = once(legacy.as_mut()).await
    {
        early = Some(result);
    }
    let livepoint = outer.delete_entered.load(Ordering::SeqCst);
    let before_second = h.world.repo.inner.find_one(h.old_target.obj_id()).await?
        == Some(h.old_target.clone().to_generic());
    if !livepoint || early.is_some() {
        // A genuine unrelated setup/deadlock error is not labelled ownership RED.
        delete_release.send(()).ok();
        anyhow::bail!("actual public legacy target-delete livepoint UNREACHED after sync release");
    }
    let before_wire = wire.attempts.load(Ordering::SeqCst);
    let mut second_sync = Box::pin(gateway.sync(actor.user_creds.user()));
    let second_poll = once(second_sync.as_mut()).await;
    let second_pending = second_poll.is_pending();
    let wire_while_paused = wire.attempts.load(Ordering::SeqCst) - before_wire;
    let owner_held = p.device_sync_guard.try_lock().is_none();
    delete_release
        .send(())
        .context("release exact target deletion")?;
    let legacy_result = legacy.await;
    wire.legacy_done.store(true, Ordering::SeqCst);
    let second_result = match second_poll {
        Poll::Ready(result) => result,
        Poll::Pending => second_sync.await,
    };

    let absent = proof::target_absent(&h).await;
    let packets = proof::packets(&h, false).await;
    let material = match interval {
        Interval::Accept => proof::publication(&h).await.is_ok(),
        Interval::Finalize => {
            let expected: HashMap<_, _> = before
                .into_iter()
                .filter(|(id, _)| id != &h.old_target.obj_id())
                .collect();
            proof::snapshot(&h).await == expected
                && outer.material_attempts() == 0
                && h.world.repo.material_attempts() == 0
        }
    };
    let calls = wire.after_legacy.lock().unwrap().clone();
    let unblocked = boundary(&first_result)
        && boundary(&second_result)
        && calls == vec![false, true]
        && wire.attempts.load(Ordering::SeqCst) == 2;
    eprintln!(
        "LEGACY_OWNER {interval:?} entry_api_pending={entry_pending} ALL_entry_repo_calls={entry_calls} entry_oldC_unchanged={entry_old_unchanged} actual_delete_livepoint={livepoint} oldC_at_livepoint={} oldC_before_second_sync={before_second} second_sync_pending={second_pending} second_wire_attempts_while_legacy_paused={wire_while_paused} SAME_sync_owner_held={owner_held} legacy_ok={} actual_oldC_absent={} actual_K1_copies_ok={} material_invariant_ok={material} SAME_sync_futures_unblocked_after_legacy={unblocked}",
        outer.old_at_entry.load(Ordering::SeqCst),
        legacy_result.is_ok(),
        absent.is_ok(),
        packets.is_ok()
    );
    ensure!(
        entry_pending
            && entry_calls == 0
            && entry_old_unchanged
            && livepoint
            && outer.old_at_entry.load(Ordering::SeqCst)
            && before_second
            && second_pending
            && wire_while_paused == 0
            && owner_held
            && unblocked,
        "public legacy interval lacks SAME sync ownership before first read or through exact target deletion"
    );
    legacy_result?;
    absent?;
    packets?;
    ensure!(
        material,
        "legacy interval changed generation/material or failed exact publication proof"
    );
    Ok(())
}
