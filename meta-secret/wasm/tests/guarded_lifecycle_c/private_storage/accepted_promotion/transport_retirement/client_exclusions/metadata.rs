use super::*;

#[derive(Clone, Copy, Debug)]
enum Damage {
    WitnessAbsent,
    OriginAbsent,
    WitnessReadErr,
}
async fn one(damage: Damage, phase: &mut &'static str) -> Result<()> {
    let (data, p, baseline) = fixture::retirement().await?;
    let witness_id = p.witness_id(&baseline.origin.request_id);
    ensure!(
        p.physical.find_one(baseline.body.obj_id()).await?
            == Some(baseline.body.clone().to_generic()),
        "actual healthy owned body missing before metadata damage"
    );
    match damage {
        Damage::WitnessAbsent => {
            p.physical.try_delete(witness_id.clone()).await?;
        }
        Damage::OriginAbsent => {
            let mut damaged = baseline.witness.clone();
            damaged.0.value.origin = None;
            p.physical.replace(damaged).await?;
        }
        Damage::WitnessReadErr => {
            *p.repo.read_fault.lock().unwrap() = Some(witness_id.clone());
        }
    }
    let actual_damaged = p.physical.find_one(witness_id.clone()).await?;
    let canonical_hash = data.hash().await?;
    let server_next = data.tail(&data.b).await?;
    *phase = "semantic_fail_closed";
    p.trace_reset();
    let first = p.sync().await;
    let second = p.sync().await; // Same actual loss/fault remains; no test restoration.
    let attempts = p.wire.writes.lock().unwrap().len();
    let noops = p.wire.noops.lock().unwrap().len();
    let hits = p.repo.read_hits.load(Ordering::SeqCst);
    let ordinary_clean = checks::ordinary_clean(&p.physical.get_db().await, &data.passes[0]);
    let still_actual = p.physical.find_one(witness_id.clone()).await?;
    let private_available = matches!(&still_actual, Some(GenericKvLogEvent::LocalGuardedPreparation(w)) if w.0.value.payload.is_none());
    eprintln!(
        "CLIENT_EXCLUSIONS metadata variant={damage:?} first_err={} retry_err={} ALL_wire_write_attempts={attempts} noop_attempts={noops} read_fault_hits={hits} ordinary_clean={ordinary_clean} actual_private_clean={private_available}",
        first.is_err(),
        second.is_err()
    );
    ensure!(
        first.is_err()
            && second.is_err()
            && attempts == 0
            && noops == 0
            && p.wire.actions.lock().unwrap().is_empty()
            && data
                .server_repo
                .find_one(baseline.origin.event_id.clone())
                .await?
                .is_none()
            && data.tail(&data.b).await? == server_next
            && data.hash().await? == canonical_hash,
        "SEMANTIC missing/unreadable proof allowed dispatch/settlement or changed server"
    );
    ensure!(
        p.repo
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| matches!(&c.operation, observer::Operation::Find(id) if *id == witness_id)),
        "selected witness production lookup not reached"
    );
    ensure!(
        ordinary_clean,
        "SEMANTIC proof loss prevented applicable ordinary tombstone purge"
    );
    ensure!(p.physical.find_one(baseline.body.obj_id()).await?.is_some_and(|e|
        matches!(e, GenericKvLogEvent::DeviceLog(b) if b.obj_id() == baseline.origin.event_id
            && !checks::action_transfer(&b.0.value, &data.passes[0]))),
        "actual redacted owned N lost or private transfers retained");
    baseline.binding(&p, false).await?;
    match damage {
        Damage::WitnessAbsent => ensure!(
            actual_damaged.is_none() && still_actual.is_none(),
            "absent indexed witness was recreated"
        ),
        Damage::OriginAbsent => ensure!(
            still_actual == actual_damaged,
            "missing origin/history reconstructed"
        ),
        Damage::WitnessReadErr => {
            ensure!(
                hits >= 2 && still_actual == actual_damaged,
                "persistent selected read error not real or inaccessible FULL changed"
            );
            // Physical FULL retained under unreadability is explicitly unavailable, never claimed purged.
            *p.repo.read_fault.lock().unwrap() = None;
            *phase = "semantic_readable_retry";
            p.trace_reset();
            let retry = p.sync().await;
            fixture::settled(&data, &p, &baseline, &retry).await?;
            ensure!(
                checks::ordinary_clean(&p.physical.get_db().await, &data.passes[0]),
                "readable retry cleanup incomplete"
            );
        }
    }
    Ok(())
}
#[tokio::test]
async fn client_exclusions_actual_metadata_loss_and_unreadable_proof_fail_closed() -> Result<()> {
    let mut outcomes = vec![];
    for mode in [
        Damage::WitnessAbsent,
        Damage::OriginAbsent,
        Damage::WitnessReadErr,
    ] {
        let mut phase = "setup";
        let result = one(mode, &mut phase).await;
        result_line(&format!("{mode:?}"), phase, &result);
        outcomes.push(result);
    }
    let failures = outcomes.iter().filter(|r| r.is_err()).count();
    ensure!(
        failures == 0,
        "CLIENT_EXCLUSIONS metadata: {failures}/3 failed; all variants collected"
    );
    Ok(())
}
