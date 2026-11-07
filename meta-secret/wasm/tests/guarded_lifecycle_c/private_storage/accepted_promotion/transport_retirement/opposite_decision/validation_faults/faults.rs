use super::*;
#[tokio::test]
async fn nine_actual_changed_rollover_cuts_resume_only_frozen_or_nondurable_attempt_as_applicable()
-> Result<()> {
    let mut results = vec![];
    for cut in [
        Cut::ComputeRead,
        Cut::IndexDrop,
        Cut::FullBefore,
        Cut::FullDrop,
        Cut::FullReadback,
        Cut::FullLostReply,
        Cut::OriginReadback,
        Cut::BodyBefore,
        Cut::BodyReadback,
    ] {
        results.push(run(cut).await);
    }
    for result in results {
        result?;
    }
    Ok(())
}
async fn run(cut: Cut) -> Result<()> {
    eprintln!("ROLLOVER CUT {cut:?} START fresh actual settled N");
    let (e, retired) = fixture::settled(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = fixture::server_hash(&e.h).await?;
    let canonical_before = digest(&canonical(&e.h.case).await?)?;
    let next = retired
        .witness
        .0
        .value
        .origin
        .as_ref()
        .unwrap()
        .event_id
        .clone()
        .next();
    e.gate.arm(cut, &e.h, &retired);
    let requests = e.h.wire.attempts.lock().unwrap().len();
    let first = e.decide().await;
    let hit = e.gate.hits.load(Ordering::SeqCst);
    let readhit = e.gate.read_hits.load(Ordering::SeqCst);
    let w = e.h.case.h.witness().await?;
    let body = e.h.physical.find_one(next.clone()).await?;
    let durable = w.0.value.payload.clone();
    let first_gate = match cut {
        Cut::FullLostReply => first
            .as_ref()
            .is_ok_and(|r| r.event_id == next && r.phase == LiveboardOperationPhase::Queued),
        Cut::BodyReadback => first
            .as_ref()
            .is_ok_and(|r| r.event_id == next && r.phase == LiveboardOperationPhase::Unknown),
        _ => first.is_err(),
    };
    let physical_gate = match cut {
        Cut::ComputeRead | Cut::IndexDrop | Cut::FullBefore | Cut::FullDrop => {
            w == retired.witness && body.is_none()
        }
        Cut::FullReadback => {
            w.0.value.phase == GuardedPreparationPhase::Staged
                && w.0.value.origin.is_none()
                && durable.is_some()
                && body.is_none()
        }
        Cut::OriginReadback | Cut::BodyBefore => {
            w.0.value.phase == GuardedPreparationPhase::Staged
                && w.0
                    .value
                    .origin
                    .as_ref()
                    .is_some_and(|o| o.event_id == next)
                && durable.is_some()
                && body.is_none()
        }
        Cut::FullLostReply | Cut::BodyReadback => {
            durable.is_some()
                && w.0
                    .value
                    .origin
                    .as_ref()
                    .is_some_and(|o| o.event_id == next)
                && body.is_some()
        }
    };
    let mut intended_index = retired.index.clone();
    if !matches!(cut, Cut::ComputeRead | Cut::IndexDrop) {
        intended_index.0.value.revision += 1;
    }
    let index_exact = e.h.case.h.index().await? == intended_index;
    let no_sends = checks::capabilities_only(&e, requests);
    let no_output = checks::no_material(&e, None);
    let raw_server = fixture::server_hash(&e.h).await? == server;
    let no_leak = first
        .as_ref()
        .err()
        .is_none_or(|x| !x.to_string().contains("ROLLOVER_PRIVATE_BACKEND"));
    eprintln!(
        "ROLLOVER CUT {cut:?} FIRST hit={hit} intended_readback_failure={readhit} first_outcome_exact={first_gate} actual_partial_state_exact={physical_gate} actual_FULL_durable={} body_durable={} server_unchanged={raw_server}",
        durable.is_some(),
        body.is_some()
    );
    let first_barriers = checks::barriers(
        &e.gate.calls.lock().unwrap(),
        &e.gate.success.lock().unwrap(),
        &e.h.case.h.state.witness_id(),
        &next,
        &retired.index,
    );
    let attempted_origin=e.gate.calls.lock().unwrap().iter().filter(|c|matches!(&c.operation,Operation::Write(_,event) if matches!(event.as_ref(),GenericKvLogEvent::LocalGuardedPreparation(w) if w.0.value.origin.as_ref().is_some_and(|o|o.event_id==next)))).count();
    let attempted_body=e.gate.calls.lock().unwrap().iter().filter(|c|matches!(&c.operation,Operation::Write(_,event) if matches!(event.as_ref(),GenericKvLogEvent::DeviceLog(b) if b.obj_id()==next))).count();
    let withheld = (!matches!(
        cut,
        Cut::ComputeRead | Cut::IndexDrop | Cut::FullBefore | Cut::FullDrop | Cut::FullReadback
    ) || attempted_origin == 0)
        && (cut != Cut::OriginReadback || attempted_body == 0);
    e.gate.clear();
    // A committed FULL body-absent stage must not be appended by ordinary sync.
    let ordinary = if durable.is_some() && body.is_none() {
        e.gate.reset();
        let before = e.h.case.h.witness().await?;
        let actual_baseline = checks::baseline_claims(&e).await?;
        let wire_before = e.h.wire.attempts.lock().unwrap().len();
        let synced = e.gateway.sync(e.h.case.h.state.b.user()).await;
        let state_same = e.h.case.h.witness().await? == before
            && e.h.physical.find_one(next.clone()).await?.is_none();
        let output = checks::ordinary_material(&e, &actual_baseline);
        let sent=e.h.wire.attempts.lock().unwrap()[wire_before..].iter().any(|r|matches!(r,SyncRequest::Write(w) if matches!(&**w,WriteSyncRequest::DeviceTransportNoop(_)) || matches!(&**w,WriteSyncRequest::Event(s) if matches!(&s.payload,SignedActionPayload::GuardedRemovalDecision(_)))));
        eprintln!(
            "ROLLOVER CUT {cut:?} ordinary_full_sync_ok={} SAME_frozen_bodyAbsent={state_same} no_auto_send={}",
            synced.is_ok(),
            !sent
        );
        output?;
        ensure!(
            state_same && !sent,
            "ordinary sync automatically queued/published Staged decision"
        );
        synced.map_err(|_| {
            anyhow::anyhow!(
                "DEPENDENCY: actual full ordinary sync failed at durable FULL/body-absent cut"
            )
        })
    } else {
        Ok(())
    };
    e.gate.reset();
    let retry_index = e.h.case.h.index().await?;
    let index_before = digest(&retry_index)?;
    let retry_requests = e.h.wire.attempts.lock().unwrap().len();
    let server_before_retry = fixture::server_hash(&e.h).await?;
    let retry = e.decide().await;
    let frozen_gate = if let Some(frozen) = &durable {
        let after = e.h.case.h.witness().await?;
        after.0.value.payload.as_ref() == Some(frozen)
            && digest(&e.h.case.h.index().await?)? == index_before
            && checks::frozen_retry(&e, frozen, &retired).is_ok()
    } else {
        true
    };
    let retry_exact = retry
        .as_ref()
        .is_ok_and(|r| r.event_id == next && r.phase == LiveboardOperationPhase::Queued);
    let final_state = checks::current(&e, &retired).await;
    let no_retry_output = checks::no_material(&e, None);
    let retry_sends = checks::capabilities_only(&e, retry_requests);
    let old = retired.witness.0.value.origin.as_ref().unwrap();
    let server_final =
        e.h.case
            .h
            .state
            .server_repo
            .find_one(next.clone())
            .await?
            .is_none()
            && digest(
                &e.h.case
                    .h
                    .state
                    .server_repo
                    .find_one(old.event_id.clone())
                    .await?
                    .context("actual unchanged old server N marker")?,
            )? == retired.marker_hash
            && digest(&canonical(&e.h.case).await?)? == canonical_before
            && fixture::server_hash(&e.h).await? == server_before_retry;
    eprintln!(
        "ROLLOVER CUT {cut:?} FINAL cleared_retry_capability_only={} actual_server_N_plus_1_empty_oldN_marker_canonical_unchanged={server_final}",
        retry_sends.is_ok()
    );

    let retry_barriers = checks::barriers(
        &e.gate.calls.lock().unwrap(),
        &e.gate.success.lock().unwrap(),
        &e.h.case.h.state.witness_id(),
        &next,
        &retry_index,
    );
    eprintln!(
        "ROLLOVER CUT {cut:?} RETRY actual_queued_N_plus_1={retry_exact} frozen_durable_stage_reused={frozen_gate} actual_current_crypto_archive_body_ok={}",
        final_state.is_ok()
    );
    // Gather reached retry evidence even when the ordinary prefix reported a dependency.
    ensure!(
        hit == 1
            && (!matches!(
                cut,
                Cut::FullReadback | Cut::OriginReadback | Cut::BodyReadback
            ) || readhit == 1),
        "named actual changed fault/readback UNREACHED"
    );
    no_sends?;
    no_output?;
    no_retry_output?;
    retry_sends?;
    ensure!(
        server_final,
        "cleared explicit retry dispatched/acknowledged Business or changed old technical/canonical proof"
    );
    first_barriers?;
    retry_barriers?;
    ensure!(
        withheld,
        "failed FULL/origin readback did not withhold next sensitive boundary"
    );
    ensure!(
        first_gate && physical_gate && index_exact && raw_server && no_leak,
        "first fault outcome/durability/privacy gate differs"
    );
    ensure!(
        retry_exact && frozen_gate,
        "cleared explicit retry did not retain actual frozen stage/identity"
    );
    final_state?;
    ordinary?;
    Ok(())
}
