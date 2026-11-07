//! Actual canonical Applied precedes real raw-source persistence uncertainty.
mod raw_repo;
mod wire;

use super::*;
use meta_secret_core::node::db::events::vault::liveboard_metadata::RemovalResultRecord;
use meta_server_node::server::server_app::ServerApp;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    Drop,
    LostReply,
}
type Case = fixture::Case<InMemKvLogEventRepo>;

#[tokio::test]
async fn canonical_applied_raw_drop_requires_same_signed_source_retry_and_actual_second_ack()
-> Result<()> {
    run(Fault::Drop).await
}
#[tokio::test]
async fn canonical_applied_durable_raw_lost_reply_advances_tail_without_received_business_ack()
-> Result<()> {
    run(Fault::LostReply).await
}

fn gateway(
    case: &Case,
    wire: Arc<wire::Forward>,
) -> SyncGateway<ExistingRepo<repo::Observed<InMemKvLogEventRepo>>, wire::Forward> {
    SyncGateway::new(
        "actual-C2-raw-source-gap".into(),
        case.h.p.clone(),
        wire,
        case.h.state.master.clone(),
    )
}
async fn canonical_result(case: &Case) -> Result<RemovalResultRecord> {
    let p = Arc::new(PersistentObject::new(case.h.state.server_repo.clone()));
    let log = PersistentVault::from(p)
        .get_vault_log_artifact(case.h.state.b.vault_name.clone())
        .await?;
    let records: Vec<_> = log
        .0
        .value
        .liveboard_removal_results
        .iter()
        .filter(|r| r.request_id == case.h.state.uuid)
        .collect();
    let [record] = records.as_slice() else {
        anyhow::bail!("actual unique canonical result absent/duplicated")
    };
    Ok((**record).clone())
}

async fn run(fault: Fault) -> Result<()> {
    let case = Case::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let (receipt, frozen) = case.prepare(true).await?;
    super::super::stage::private_crypto(&case.h.state, &case.h.witness().await?)?;
    let e = case.observations.expected()?;
    let body = case
        .h
        .p
        .repo
        .inner
        .inner
        .find_one(receipt.event_id.clone())
        .await?
        .context("actual local queued B source")?;
    let local_before = case.h.p.repo.snapshot().await?;
    ensure!(
        case.h
            .state
            .server_repo
            .find_one(e.event_id.clone())
            .await?
            .is_none(),
        "actual guarded raw N must initially be absent"
    );
    let raw = Arc::new(raw_repo::RawRepo::new(case.h.state.server_repo.clone()));
    // Original server master + SAME backing Arc. Old server receives no calls from here onward.
    let server = Arc::new(ServerApp::new(
        raw.clone(),
        case.h.state.server_master.clone(),
    )?);
    server.init().await?;
    raw.arm(e.clone(), body.clone(), fault, case.observations.clone());
    let wire = Arc::new(wire::Forward {
        local: case.h.p.repo.inner.clone(),
        server,
        observations: case.observations.clone(),
        post_error_vault: Mutex::new(vec![]),
        request_sequence: AtomicUsize::new(0),
        source_error_order: AtomicUsize::new(0),
        source_error: Mutex::new(None),
    });
    let sync = gateway(&case, wire.clone())
        .sync(case.h.state.b.user())
        .await;
    let sent = case.observations.sent.lock().unwrap().clone();
    let action = sent
        .first()
        .context("actual first signed Guarded send")?
        .action
        .clone();
    let server_applied = proof::applied(case.h.state.server_repo.clone(), &e, &action).await?;
    let local_applied = proof::applied(case.h.p.repo.inner.inner.clone(), &e, &action).await?;
    let actual_raw = case
        .h
        .state
        .server_repo
        .find_one(e.event_id.clone())
        .await?;
    let partial = case.h.witness().await?;
    let hits = raw.hits.load(Ordering::SeqCst);
    let readbacks = raw.raw_reads.load(Ordering::SeqCst);
    let original_error_preserved =
        wire.source_error
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|captured| {
                sync.as_ref().err().is_some_and(|returned| {
                    returned
                        .chain()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        == *captured
                })
            });
    eprintln!("C3 raw {fault:?} actual_original_source_error_preserved={original_error_preserved}");
    {
        let post_error_vault = wire.post_error_vault.lock().unwrap();
        let post_error_successes = post_error_vault
            .iter()
            .filter(|read| read.response_success)
            .count();
        let post_error_exact_sources = post_error_vault
            .iter()
            .filter(|read| {
                read.exact_unique_source
                    && read.source_id == e.event_id
                    && read.source_error_order > 0
                    && read.read_order > read.source_error_order
            })
            .count();
        eprintln!(
            "C3 raw {fault:?} posterror_actual_Vault_reads={} successful_Data={} exact_unique_ownApplied_sources={post_error_exact_sources} localApplied={local_applied}",
            post_error_vault.len(),
            post_error_successes
        );
        eprintln!(
            "C2 raw {fault:?} first_sync_ok={} actual_fault_hits={hits} applied_before_raw_save={} server_applied={server_applied} local_applied={local_applied} raw_present={} source_sends={} received_business_acks={} actual_raw_readbacks={readbacks}",
            sync.is_ok(),
            raw.applied_before_fault.load(Ordering::SeqCst),
            actual_raw.is_some(),
            sent.len(),
            sent.iter().filter(|s| s.server_ok).count()
        );
        ensure!(
            hits == 1 && raw.applied_before_fault.load(Ordering::SeqCst),
            "actual raw-save fault AFTER unique canonical Applied UNREACHED"
        );
        ensure!(
            sync.is_err()
                && original_error_preserved
                && server_applied
                && local_applied
                && post_error_vault.len() == 1
                && post_error_successes == 1
                && post_error_exact_sources == 1
                && sent.len() == 1
                && sent.iter().all(|s| s.durable_signature && !s.server_ok),
            "original source error must retain zero ACK while one actual posterror Vault response imports exact own Applied"
        );
    }
    ensure!(
        partial.0.value.phase == GuardedPreparationPhase::Signed
            && partial.0.value.payload.as_ref() == Some(&frozen)
            && case.observations.live.lock().unwrap().is_empty()
            && case.observations.claim_writes.lock().unwrap().is_empty(),
        "source error published/scrubbed frozen stage before ordinary successful continuation"
    );
    super::super::stage::private_crypto(&case.h.state, &partial)?;
    let signature = partial
        .0
        .value
        .origin
        .as_ref()
        .and_then(|o| o.business_signature.clone())
        .context("original pre-send durable compact signature")?;
    let canonical = canonical_result(&case).await?;
    match fault {
        Fault::Drop => ensure!(
            actual_raw.is_none() && readbacks > 0,
            "save_verified did not catch actual silent raw Drop by equality readback"
        ),
        Fault::LostReply => ensure!(
            actual_raw == Some(body.clone()),
            "actual raw LostReply did not commit exact original N before Err"
        ),
    }
    raw.armed.store(false, Ordering::SeqCst);
    let fresh_tail = wire
        .send(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
            ServerTailRequest {
                sender: case.h.state.b.user(),
                producer_reference: None,
            },
        ))))
        .await?
        .to_server_tail()?;
    eprintln!(
        "C2 raw {fault:?} fresh_actual_source_n={} first_typed_id={} tail_curr={:?}",
        e.event_id.id.curr,
        e.event_id == e.event_id.clone().first(),
        fresh_tail.device_log_tail.as_ref().map(|id| id.id.curr)
    );
    match fault {
        Fault::Drop => ensure!(
            fresh_tail
                .device_log_tail
                .as_ref()
                .map_or(e.event_id == e.event_id.clone().first(), |id| id
                    == &e.event_id),
            "fresh actual tail must still request original missing N"
        ),
        Fault::LostReply => ensure!(
            fresh_tail.device_log_tail == Some(e.event_id.clone().next()),
            "fresh actual tail must advance past the already durable raw N"
        ),
    }
    eprintln!(
        "C2 raw {fault:?} fresh_actual_tail_verified=true canonical_result_unchanged={} received_ack_not_inferred_from_tail=true",
        canonical_result(&case).await? == canonical
    );
    ensure!(
        canonical_result(&case).await? == canonical,
        "tail query changed actual canonical winner/source"
    );
    let retry = gateway(&case, wire).sync(case.h.state.b.user()).await;
    let final_sent = case.observations.sent.lock().unwrap().clone();
    let final_server_applied =
        proof::applied(case.h.state.server_repo.clone(), &e, &action).await?;
    let final_local_applied =
        proof::applied(case.h.p.repo.inner.inner.clone(), &e, &action).await?;
    let expected_sends = if fault == Fault::Drop { 2 } else { 1 };
    let expected_acks = usize::from(fault == Fault::Drop);
    let acks = final_sent.iter().filter(|s| s.server_ok).count();
    eprintln!(
        "C2 raw {fault:?} retry_ok={} total_source_sends={} received_business_acks={acks} server_applied={final_server_applied} local_applied={final_local_applied} noops={}",
        retry.is_ok(),
        final_sent.len(),
        case.observations.noops.load(Ordering::SeqCst)
    );
    ensure!(
        retry.is_ok()
            && final_server_applied
            && final_local_applied
            && final_sent.len() == expected_sends
            && acks == expected_acks
            && final_sent
                .iter()
                .all(|s| s.action == action && s.durable_signature)
            && case.observations.noops.load(Ordering::SeqCst) == 0,
        "same-source retry/resend/received-ACK distinction differs"
    );
    // Existing common() intentionally remains unused: it requires sent1 AND ACKtrue.
    proof::published(&case, &frozen).await?;
    let completed = case.h.witness().await?;
    ensure!(
        completed
            .0
            .value
            .origin
            .as_ref()
            .and_then(|o| o.business_signature.as_ref())
            == Some(&signature)
            && signature.signed_action_hash == proof::hash(&action)?
            && completed.0.value.request_id == e.uuid
            && case
                .h
                .p
                .repo
                .inner
                .inner
                .find_one(e.event_id.clone())
                .await?
                == Some(body.clone())
            && case
                .h
                .state
                .server_repo
                .find_one(e.event_id.clone())
                .await?
                == Some(body)
            && canonical_result(&case).await? == canonical
            && raw.hits.load(Ordering::SeqCst) == hits,
        "continuation changed original source/signature/frozen identity/canonical result"
    );
    let local_after = case.h.p.repo.snapshot().await?;
    let bodies = |db: &super::super::super::fixture::Snapshot| {
        db.iter()
            .filter(|(id, x)| {
                id.fqdn == e.event_id.fqdn && matches!(x, GenericKvLogEvent::DeviceLog(_))
            })
            .map(|(id, x)| (id.clone(), x.clone()))
            .collect::<std::collections::HashMap<_, _>>()
    };
    ensure!(
        bodies(&local_before) == bodies(&local_after),
        "raw-gap retry queued new source/N+1"
    );
    eprintln!(
        "C2 raw {fault:?} exact_original_body_sig_frozen_generation=true unique_canonical_result_retained=true complete_private_scrub=true"
    );
    Ok(())
}
