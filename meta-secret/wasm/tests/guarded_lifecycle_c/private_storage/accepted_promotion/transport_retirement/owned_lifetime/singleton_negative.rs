//! Three return-only proof controls on the same real public removal lifecycle.
use super::*;
#[path = "singleton_negative_repo.rs"]
mod singleton_negative_repo;
use meta_secret_core::node::common::model::liveboard_producer::{
    LiveboardProducerReceipt as Receipt, ProducerReference, ProducerSourceObservation,
};
use singleton_negative_repo::{Mode, Repo, no_effects, same_receipt};
#[tokio::test]
async fn singleton_missing_prepared_history_stays_fail_closed() -> Result<()> {
    run(Mode::MissingPrepared).await
}

#[tokio::test]
async fn singleton_wrong_prepared_base_stays_fail_closed() -> Result<()> {
    run(Mode::WrongBase).await
}

#[tokio::test]
async fn singleton_nonpublished_full_cannot_use_terminal_path() -> Result<()> {
    run(Mode::SignedFull).await
}

async fn run(mode: Mode) -> Result<()> {
    let h = published_owner().await?;
    ready_show(&h).await?;
    let old = h.case.h.witness().await?;
    ensure!(
        old.0.value.phase == GuardedPreparationPhase::AcceptedPublished
            && old.0.value.payload.is_none(),
        "SETUP exact Published/None required"
    );
    let full = {
        let reads = h.repo.reads.lock().unwrap();
        reads
            .iter()
            .find_map(|read| match &read.event {
                Some(GenericKvLogEvent::LocalGuardedPreparation(w))
                    if w.obj_id() == old.obj_id()
                        && w.0.value.phase == GuardedPreparationPhase::Signed
                        && w.0.value.payload.is_some()
                        && w.0
                            .value
                            .origin
                            .as_ref()
                            .is_some_and(|o| o.business_signature.is_some()) =>
                {
                    Some(w.clone())
                }
                _ => None,
            })
            .context("SETUP genuinely captured Signed/FULL production read absent")?
    };
    ensure!(
        full.0.value.origin == old.0.value.origin
            && full.0.value.owner == old.0.value.owner
            && full.0.value.base_vault == old.0.value.base_vault
            && full.0.value.survivors == old.0.value.survivors,
        "SETUP actual Signed/FULL belongs to another attempt"
    );
    let (before, before_log) = super::super::super::canonical(&h.case).await?;
    let old_result = before_log
        .0
        .value
        .liveboard_removal_results
        .iter()
        .find(|r| r.request_id == h.case.h.state.uuid)
        .context("SETUP original Applied result")?
        .clone();
    ensure!(
        old_result.application_state == RemovalApplicationState::Applied
            && old_result.outcome == DecisionOutcome::Accepted
            && old_result
                .source
                .as_ref()
                .is_some_and(|s| old.0.value.origin.as_ref().is_some_and(|o| s.event_id
                    == o.event_id
                    && s.signer == *h.case.h.state.b.device_id()
                    && o.business_signature
                        .as_ref()
                        .is_some_and(|b| b.signed_action_hash == s.signed_action_hash))),
        "SETUP original Guarded Applied source differs"
    );
    let prefix = PersistentDeviceLog::from(h.p.clone())
        .find_tail_id(&h.case.h.state.b.user_id())
        .await?
        .context("SETUP actual A tail")?;
    let wrapper = Arc::new(Repo {
        inner: h.repo.clone(),
        mode,
        old: old.clone(),
        before: before.clone(),
        full,
        request_id: prefix.clone().next(),
        hits: Mutex::new(vec![]),
        enabled: true.into(),
    });
    let p = Arc::new(PersistentObject {
        repo: wrapper.clone(),
        device_log_append_guard: h.p.device_log_append_guard.clone(),
        client_operation_guard: h.p.client_operation_guard.clone(),
        device_sync_guard: h.p.device_sync_guard.clone(),
    });
    ensure!(
        Arc::ptr_eq(&p.client_operation_guard, &h.p.client_operation_guard)
            && Arc::ptr_eq(&p.device_sync_guard, &h.p.device_sync_guard)
            && Arc::ptr_eq(&p.device_log_append_guard, &h.p.device_log_append_guard),
        "SETUP wrapper must retain SAME three owners"
    );
    let service = MetaClientService {
        data_transfer: Arc::new(MetaClientDataTransfer {
            dt: MetaClientRequestTransfer::new(),
        }),
        sync_gateway: Arc::new(SyncGateway::new(
            "singleton-proof-control".into(),
            p.clone(),
            h.wire.clone(),
            h.case.h.state.master.clone(),
        )),
        state_provider: Arc::new(MetaClientStateProvider::new()),
        p_obj: p,
        device_data: h.case.h.state.b.device(),
        master_key: h.case.h.state.master.clone(),
    };
    let start = h.repo.tick();
    let receipt = service
        .remove_member_checked(h.case.h.state.a.device_id().to_string())
        .await?;
    let returned = h.repo.tick();
    let Some(ProducerDecisionStage::Attempted {
        source: u_source, ..
    }) = &receipt.decision_stage
    else {
        anyhow::bail!("SETUP actual independent U not attempted")
    };
    let (r, _, _) = singleton_proof::actual_source(&h, &receipt.request_stage.source).await?;
    let (u, signed_u, _) = singleton_proof::actual_source(&h, u_source).await?;
    ensure!(
        r.obj_id() == prefix.clone().next() && u.obj_id() == r.obj_id().next(),
        "actual selected R/U fullIDs changed"
    );
    let (vault, log) = super::super::super::canonical(&h.case).await?;
    let uuid = singleton_proof::later_uuid(&h, &before, &vault, &log, &r, &u, &signed_u).await?;
    let a = &h.case.h.state.b;
    let b = &h.case.h.state.a;
    let result = log
        .0
        .value
        .liveboard_removal_results
        .iter()
        .find(|r| r.request_id == uuid)
        .context("actual later Applied result")?;
    ensure!(
        vault.obj_id() == before.obj_id().next()
            && vault.0.value.members().len() == 1
            && vault.0.value.members()[0].user_data == a.user()
            && matches!(vault.0.value.find_user(b.device_id()), Some(UserMembership::Outsider(o))
            if o.user_data == b.user() && o.status == UserDataOutsiderStatus::Declined)
            && result.application_state == RemovalApplicationState::Applied
            && result.outcome == DecisionOutcome::Accepted
            && result.source.is_none()
            && result.requester_id == *a.device_id()
            && result.responder_id == *a.device_id()
            && result.target_id == *b.device_id()
            && log.0.value.liveboard_removal_results.contains(&old_result),
        "actual real singleton/Applied roles/sourceNone/old result absent"
    );
    let persistent = PersistentVault::from(h.p.clone());
    let mut public_log = log.clone();
    public_log.0.value = public_log.0.value.without_private_producer_metadata();
    ensure!(
        persistent.get_vault(a.vault_name.clone()).await? == vault
            && persistent
                .get_vault_log_artifact(a.vault_name.clone())
                .await?
                == public_log,
        "actual physical singleton import must precede selected fault"
    );
    // Physical diagnostic reads above do not enter the outer wrapper's hit ledger.
    let first = wrapper.evidence()?;
    let returned_faults = { wrapper.hits.lock().unwrap().clone() };
    for hit in &returned_faults {
        ensure!(
            h.repo.physical.find_one(hit.id.clone()).await?.as_ref() == Some(&hit.original),
            "return-only fault changed the physical original record"
        );
    }
    ensure!(
        first.order > start && first.order < returned,
        "selected returned-fault evidence outside actual operation"
    );
    {
        let writes = h.repo.writes.lock().unwrap();
        ensure!(
            writes.iter().any(|w| w.done < first.order
                && w.forward > start
                && w.success
                && w.exact
                && w.event == vault.clone().to_generic()),
            "selected fault preceded actual production singleton import"
        );
    }
    ensure!(
        receipt.phase == ProducerPhase::Unknown
            && receipt.error_code.as_deref() == Some("OperationUnavailable")
            && receipt.canonical_state == ProducerCanonicalState::Unknown
            && receipt.material_stage == Some(ProducerMaterialStage::Ready),
        "reached invalid proof must stay unavailable, not successful"
    );
    ensure!(
        h.case.h.witness().await? == old,
        "invalid proof changed physical old witness"
    );
    no_effects(&h, &old, &r, &u, start, first.order, returned)?;
    let own = ArtifactId::from(DeviceLogDescriptor::from(a.user_id())).fqdn;
    let after: Vec<_> = h
        .local
        .inner
        .get_db()
        .await
        .into_values()
        .filter(|e| {
            matches!(e, GenericKvLogEvent::DeviceLog(_))
                && e.obj_id().fqdn == own
                && e.obj_id().id.curr > prefix.id.curr
        })
        .collect();
    ensure!(
        after.len() == 2
            && after.contains(&r.clone().to_generic())
            && after.contains(&u.clone().to_generic()),
        "invalid proof added/replaced a command"
    );
    let hits = returned_faults.len();
    let group = match mode {
        Mode::MissingPrepared => "missingPrepared",
        Mode::WrongBase => "wrongBase",
        Mode::SignedFull => "signedFull",
    };
    eprintln!(
        "SINGLETON NEGATIVE group={group} real_Applied1=true selected_outer_hits={hits} returned_fault=true witness_unchanged=true terminal_FULL_old_output_attempts=0"
    );
    if mode == Mode::MissingPrepared {
        wrapper.enabled.store(false, Ordering::SeqCst);
        cleared_context(&h, &wrapper, &service, &receipt, &uuid, &r, &u).await?;
    }
    Ok(())
}
async fn cleared_context(
    h: &harness::Harness,
    wrapper: &Repo,
    service: &MetaClientService<Repo, wire::Forward>,
    receipt: &Receipt,
    uuid: &str,
    r: &DeviceLogObject,
    u: &DeviceLogObject,
) -> Result<()> {
    use meta_secret_core::node::db::objects::local_removal_envelope::PersistentRemovalEnvelope;
    let Some(ProducerDecisionStage::Attempted { source, .. }) = &receipt.decision_stage else {
        anyhow::bail!("actual existing U source absent")
    };
    let reference = ProducerReference {
        version: receipt.version,
        kind: receipt.kind.clone(),
        request_id: Some(uuid.to_owned()),
        request_stage: receipt.request_stage.source.clone(),
        decision_stage: Some(source.clone()),
    };
    let same = |value: &Receipt| same_receipt(value, receipt, source, uuid);
    let physical_before = h.local.inner.get_db().await;
    let read_start = h.repo.tick();
    let observed = service
        .reconcile_producer_receipt(reference.clone())
        .await
        .context("SINGLETON_RESUME_RECONCILE")?;
    let read_done = h.repo.tick();
    ensure!(
        same(&observed) && h.local.inner.get_db().await == physical_before,
        "actual read-only reconcile changed state or exact Applied R/U"
    );
    {
        let calls = h.repo.calls.lock().unwrap();
        ensure!(
            calls
                .iter()
                .filter(|c| c.order > read_start && c.order < read_done)
                .all(|c| matches!(&c.op, repo::Op::Find(_) | repo::Op::Key(_))),
            "read-only reconcile attempted local mutation"
        );
        let wire = h.wire.calls.lock().unwrap();
        let window: Vec<_> = wire
            .iter()
            .filter(|c| c.order > read_start && c.order < read_done)
            .collect();
        ensure!(
            matches!(window.as_slice(), [call] if matches!(&call.request, SyncRequest::Read(q)
            if matches!(&**q, ReadSyncRequest::ServerTail(t) if t.producer_reference.as_ref() == Some(&reference)))),
            "reconcile must make exactly one same-reference observation"
        );
    }
    let continued = service
        .continue_local_remove_checked(reference)
        .await
        .context("SINGLETON_RESUME_CONTINUE")?;
    let completed = h.repo.tick();
    ensure!(
        same(&continued) && h.case.h.witness().await? == wrapper.old,
        "explicit continue must complete same R/U while old witness remains Published/None"
    );
    let registry = PersistentRemovalEnvelope { p_obj: h.p.clone() }
        .load(&h.case.h.state.b.user_id())
        .await?
        .context("actual cleared registry absent")?;
    ensure!(
        registry.0.value.producer_context.is_none(),
        "actual explicit continue did not clear context"
    );
    let clear = registry.clone().to_generic();
    {
        let writes = h.repo.writes.lock().unwrap();
        let reads = h.repo.reads.lock().unwrap();
        ensure!(
            writes.iter().any(|w| w.forward > read_done
                && w.done < completed
                && w.success
                && w.exact
                && w.event == clear
                && reads.iter().any(|q| q.order > w.done
                    && q.order < completed
                    && q.event.as_ref() == Some(&clear))),
            "actual clear whole PRODUCTION readback absent before continue return"
        );
    }
    let hit = wrapper.evidence()?.order;
    no_effects(h, &wrapper.old, r, u, hit - 1, hit, completed)?;
    let marker = (
        h.repo.calls.lock().unwrap().len(),
        h.repo.writes.lock().unwrap().len(),
        h.wire.calls.lock().unwrap().len(),
    );
    let sync_start = h.repo.tick();
    service
        .sync_gateway
        .sync(h.case.h.state.b.user())
        .await
        .context("SINGLETON_FIRST_CONTEXT_NONE_SYNC")?;
    let sync_returned = h.repo.tick();
    let mut terminal = wrapper.old.clone();
    terminal.0.value.phase = GuardedPreparationPhase::BlockedRetiredCommand;
    let intended = terminal.clone().to_generic();
    {
        let calls = h.repo.calls.lock().unwrap();
        let writes = h.repo.writes.lock().unwrap();
        let reads = h.repo.reads.lock().unwrap();
        let first = writes
            .iter()
            .find(|w| {
                w.forward > sync_start
                    && w.done < sync_returned
                    && w.success
                    && w.exact
                    && w.event == intended
                    && calls.iter().any(|c| {
                        c.order > sync_start
                            && c.order < w.forward
                            && matches!(&c.op, repo::Op::Write(true, e) if **e == intended)
                    })
            })
            .context("first terminal existing-only replacement with absent context missing")?;
        ensure!(
            reads.iter().any(|q| q.order > sync_start
                && q.order < first.forward
                && q.event.as_ref() == Some(&clear))
                && reads.iter().any(|q| q.order > first.done
                    && q.order < sync_returned
                    && q.event.as_ref() == Some(&intended)),
            "first context-absent terminal PRODUCTION read/readback ordering absent"
        );
    }
    ensure!(
        h.case.h.witness().await? == terminal,
        "first context-absent terminal clone differs"
    );
    h.sync().await.context("SINGLETON_CONTEXT_NONE_REPEAT")?;
    singleton_proof::ready_singleton(h, &wrapper.old, &terminal, r, u, marker).await?;
    let after = PersistentRemovalEnvelope { p_obj: h.p.clone() }
        .load(&h.case.h.state.b.user_id())
        .await?
        .context("actual later registry absent")?;
    ensure!(
        after.0.value.producer_context.is_none() && !wrapper.enabled.load(Ordering::SeqCst),
        "context or disabled fault resurrected after public singleton checks"
    );
    singleton_negative_repo::no_resubmit(h, hit, h.repo.tick())?;
    eprintln!(
        "SINGLETON context_cleared_while_Published=true first_context_absent_terminal=true exact_readback=true no_resubmit=true"
    );
    Ok(())
}
