use super::*;
pub(super) fn compact(old: &LocalGuardedPreparationObject) -> Result<GuardedCompactAttempt> {
    let x = &old.0.value;
    Ok(GuardedCompactAttempt {
        origin: x.origin.clone().context("actual old N")?,
        transport_noop: x.transport_noop.clone(),
        phase: x.phase,
        base_vault: x.base_vault.clone(),
        requester_id: x.requester_id.clone(),
        target_id: x.target_id.clone(),
        survivors: x.survivors.clone(),
        claims: x.claims.clone(),
        packets: x.packets.clone(),
    })
}
pub(super) fn capabilities_only<R: KvLogEventRepo>(e: &fixture::Env<R>, from: usize) -> Result<()> {
    let requests = e.h.wire.attempts.lock().unwrap();
    let calls = &requests[from..];
    ensure!(!calls.is_empty() && calls.iter().all(|r|matches!(r,SyncRequest::Read(r) if matches!(&**r,ReadSyncRequest::LiveboardCapabilities(_)))),
        "checked explicit action unexpectedly sent domain/Noop or bypassed real capability call");
    Ok(())
}
pub(super) fn no_writes<R: KvLogEventRepo>(e: &fixture::Env<R>) -> Result<()> {
    let calls = e.gate.calls.lock().unwrap();
    let mutations = calls
        .iter()
        .filter_map(|c| match &c.operation {
            Operation::Write(_, e) => Some(e.obj_id()),
            Operation::Delete(id) | Operation::TryDelete(id) => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    ensure!(
        mutations.is_empty()
            && calls
                .iter()
                .all(|c| !matches!(&c.operation, Operation::Clean)),
        "malformed history reached ANY append/stage/index/output/deletion write"
    );
    ensure!(
        calls.windows(2).all(|w| w[0].order < w[1].order),
        "outer ALL-method trace order differs"
    );
    Ok(())
}
pub(super) fn no_material<R: KvLogEventRepo>(
    e: &fixture::Env<R>,
    planned: Option<&GuardedPreparationPayload>,
) -> Result<()> {
    let calls = e.gate.calls.lock().unwrap();
    let mut packets = 0;
    let mut device_claims = 0;
    let mut aggregates = 0;
    for call in calls.iter() {
        if let Operation::Write(_, event) = &call.operation {
            match event.as_ref() {
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(p)) => {
                    packets += 1;
                    ensure!(
                        planned.is_some_and(|f| f
                            .passes
                            .iter()
                            .all(|x| x.claim.dist_claim_id != p.value.claim_id)),
                        "unadmitted attempt published/recreated Distribution"
                    );
                }
                GenericKvLogEvent::SsDeviceLog(c) => {
                    device_claims += 1;
                    ensure!(
                        planned
                            .is_some_and(|f| f.passes.iter().all(|x| x.claim.id != c.0.value.id)),
                        "unadmitted attempt published Split claim"
                    );
                }
                GenericKvLogEvent::SsLog(log) => {
                    aggregates += 1;
                    ensure!(
                        planned.is_some_and(|f| f.passes.iter().all(|p| !log
                            .0
                            .value
                            .claims
                            .contains_key(&p.claim.id))),
                        "unadmitted attempt published aggregate Split"
                    );
                }
                _ => {}
            }
        }
    }
    eprintln!(
        "ROLLOVER ALLattempts Distribution={packets} SsDeviceLog={device_claims} SsLog={aggregates} no_planned_generation_publication=true"
    );
    Ok(())
}
pub(super) fn frozen_retry<R: KvLogEventRepo>(
    e: &fixture::Env<R>,
    frozen: &GuardedPreparationPayload,
    retired: &Retired,
) -> Result<()> {
    let ss = ArtifactId::from(SsLogDescriptor::from(e.h.case.h.state.b.vault_name.clone())).fqdn;
    let index = e.h.case.h.state.index_id();
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
    for c in e.gate.calls.lock().unwrap().iter() {
        match &c.operation {
            Operation::Read(id) | Operation::Key(id) => ensure!(
                id.fqdn != ss,
                "frozen FULL retry re-entered actual compute SsLog seam"
            ),
            Operation::Write(_, event) => match event.as_ref() {
                GenericKvLogEvent::LocalGuardedPreparationIndex(x) => ensure!(
                    x.obj_id() != index,
                    "frozen FULL retry probed EXISTING index again"
                ),
                GenericKvLogEvent::LocalGuardedPreparation(w) => ensure!(
                    w.0.value.payload.as_ref() == Some(frozen)
                        && w.0.value.previous_attempts == vec![compact(&retired.witness)?],
                    "frozen retry changed crypto/generation/archive"
                ),
                GenericKvLogEvent::DeviceLog(b) => {
                    ensure!(b.obj_id() == next, "frozen retry appended another full ID")
                }
                _ => {}
            },
            _ => {}
        }
    }
    Ok(())
}
pub(super) async fn current<R: KvLogEventRepo>(
    e: &fixture::Env<R>,
    retired: &Retired,
) -> Result<Queued> {
    let w = e.h.case.h.witness().await?;
    super::super::crypto::accepted(&e.h, &w).await?;
    let payload =
        w.0.value
            .payload
            .as_ref()
            .context("actual frozen current FULL")?;
    let old = retired.witness.0.value.origin.as_ref().unwrap();
    let next = old.event_id.clone().next();
    ensure!(
        w.0.value
            .origin
            .as_ref()
            .is_some_and(|o| o.event_id == next && o.accepted)
            && w.0.value.previous_attempts == vec![compact(&retired.witness)?]
            && w.0.value.phase == GuardedPreparationPhase::Queued
            && e.h.physical.find_one(next.clone().next()).await?.is_none(),
        "cleared retry did not converge on single actual queued N+1"
    );
    let body =
        e.h.physical
            .find_one(next.clone())
            .await?
            .context("actual N+1 body")?
            .device_log()?;
    ensure!(
        producer_source_commitment(&body)? == w.0.value.origin.as_ref().unwrap().body_commitment
            && digest(
                &e.h.physical
                    .find_one(old.event_id.clone())
                    .await?
                    .context("actual N old redacted body")?
            )? == retired.body_hash,
        "old N or new body commitment differs"
    );
    let packet_hashes = payload
        .passes
        .iter()
        .flat_map(|p| p.distributions.iter())
        .map(|p| Ok((p.obj_id(), digest(p)?)))
        .collect::<Result<HashMap<_, _>>>()?;
    // Registry is append-only: actual production tail, not stale descriptor base slot.
    let bound = PersistentRemovalEnvelope {
        p_obj: e.h.p.clone(),
    }
    .load(&e.h.case.h.state.b.user_id())
    .await?
    .context("actual registry tail")?;
    let registry = bound.clone().to_generic();
    let entries = &bound.0.value.entries;
    ensure!(
        [old.event_id.clone(), next.clone()].iter().all(|id| entries
            .iter()
            .filter(|entry| entry.event_id == *id
                && entry.mode == RemovalEnvelopeMode::Guarded
                && entry.request_id.as_deref() == Some(e.h.case.h.state.uuid.as_str()))
            .count()
            == 1)
            && entries
                .iter()
                .filter(|entry| entry.request_id.as_deref() == Some(e.h.case.h.state.uuid.as_str()))
                .count()
                == 2,
        "retry lost old N or exact Guarded SAMEUUID/N+1 entry, downgraded Legacy or allocated another identity"
    );
    Ok(Queued {
        retired: Retired {
            witness: retired.witness.clone(),
            index: retired.index.clone(),
            body_hash: retired.body_hash.clone(),
            marker_hash: retired.marker_hash.clone(),
        },
        witness_hash: digest(&w)?,
        payload_hash: digest(payload)?,
        index_hash: digest(&e.h.case.h.index().await?)?,
        registry_hash: digest(&registry)?,
        body_hash: digest(&body)?,
        packet_hashes,
        event_id: next,
    })
}
pub(super) async fn reject<R: KvLogEventRepo>(
    e: &fixture::Env<R>,
    mutant: &LocalGuardedPreparationObject,
    server: &str,
    label: &str,
) -> Result<()> {
    let index = digest(&e.h.case.h.index().await?)?;
    let requests = e.h.wire.attempts.lock().unwrap().len();
    e.gate.reset();
    let actual = e.decide().await;
    let unchanged = e.h.case.h.witness().await? == *mutant
        && digest(&e.h.case.h.index().await?)? == index
        && fixture::server_hash(&e.h).await? == server;
    let writes = no_writes(e);
    let sends = capabilities_only(e, requests);
    eprintln!(
        "ROLLOVER HISTORY {label} actual_public_rejected={} ALLwrites_absent={} domain_Noop_absent={} physical_mutant_server_unchanged={unchanged}",
        actual.is_err(),
        writes.is_ok(),
        sends.is_ok()
    );
    writes?;
    sends?;
    ensure!(unchanged, "negative mutated fixed records or actual server");
    ensure!(
        actual.is_err(),
        "HISTORY: malformed local attempt accepted by actual checked FULL retry"
    );
    Ok(())
}

/// Successes are actual OUTER returned reads, not physical adapter probes.
pub(super) fn barriers(
    calls: &[Call],
    reads: &[Success],
    witness: &ArtifactId,
    next: &ArtifactId,
    index: &LocalGuardedPreparationIndexObject,
) -> Result<()> {
    let mut changed = index.clone();
    changed.0.value.revision = changed
        .0
        .value
        .revision
        .checked_add(1)
        .context("checked actual revision")?;
    let probes = calls
        .iter()
        .filter(|c| {
            matches!(
                &c.operation,
                Operation::Write(_, event) if matches!(event.as_ref(), GenericKvLogEvent::LocalGuardedPreparationIndex(_))
            )
        })
        .collect::<Vec<_>>();
    ensure!(probes.len()<=1 && probes.iter().all(|p|matches!(&p.operation,Operation::Write(true,event) if matches!(event.as_ref(),GenericKvLogEvent::LocalGuardedPreparationIndex(x) if x==&changed))),
        "index barrier used save/enroll/fallback/wrong revision/key/UUIDlist");
    for c in calls {
        if let Operation::Write(replace, event) = &c.operation
            && let GenericKvLogEvent::LocalGuardedPreparation(w) = event.as_ref()
        {
            ensure!(
                *replace && w.obj_id() == *witness,
                "rollover private write is not SAME existing-key replace"
            );
            if w.0.value.origin.is_none() && w.0.value.payload.is_some() {
                ensure!(
                    probes.iter().any(|p| p.order < c.order
                        && reads.iter().any(|r| r.order > p.order
                            && r.order < c.order
                            && r.event == changed.clone().to_generic())),
                    "changed EXISTING index exact OUTER readback did not precede FULL install"
                );
            }
            if w.0
                .value
                .origin
                .as_ref()
                .is_some_and(|o| o.event_id == *next)
                && w.0.value.phase == GuardedPreparationPhase::Staged
            {
                let mut intended = w.clone();
                intended.0.value.origin = None;
                let whole = intended.to_generic();
                let install = calls.iter().find(|p| {
                    p.order < c.order
                        && matches!(&p.operation,Operation::Write(true,e) if **e==whole)
                });
                ensure!(
                    reads.iter().any(|r| r.order < c.order
                        && install.is_none_or(|p| r.order > p.order)
                        && r.event == whole),
                    "WHOLE intended FULL actual successful readback after install did not precede FIRST origin attach"
                );
            }
        }
        if let Operation::Write(_, event) = &c.operation
            && let GenericKvLogEvent::DeviceLog(b) = event.as_ref()
        {
            ensure!(b.obj_id()==*next && reads.iter().any(|r|r.order<c.order && matches!(&r.event,GenericKvLogEvent::LocalGuardedPreparation(w)
            if w.obj_id()==*witness && w.0.value.origin.as_ref().is_some_and(|o|o.event_id==*next && producer_source_commitment(b).is_ok_and(|hash|hash==o.body_commitment)))),
            "actual origin exact OUTER readback did not precede current body attempt");
        }
    }
    Ok(())
}
fn claim_identity(c: &meta_secret_core::node::common::model::secret::SsClaim) -> Result<String> {
    digest(&(
        &c.id,
        &c.dist_claim_id,
        &c.vault_name,
        &c.sender,
        &c.distribution_type,
        &c.receivers,
    ))
}
pub(super) async fn baseline_claims<R: KvLogEventRepo>(
    e: &fixture::Env<R>,
) -> Result<HashMap<String, String>> {
    let mut baseline = HashMap::new();
    for event in e.h.case.h.p.repo.snapshot().await?.into_values() {
        let claims = match event {
            GenericKvLogEvent::SsDeviceLog(c) => vec![c.0.value],
            GenericKvLogEvent::SsLog(l) => l.0.value.claims.into_values().collect(),
            _ => vec![],
        };
        for c in claims {
            baseline.insert(c.id.wasm_id_str(), claim_identity(&c)?);
        }
    }
    Ok(baseline)
}
pub(super) fn ordinary_material<R: KvLogEventRepo>(
    e: &fixture::Env<R>,
    baseline: &HashMap<String, String>,
) -> Result<()> {
    use meta_secret_core::node::common::model::secret::SecretDistributionType;
    let calls = e.gate.calls.lock().unwrap();
    let mut packets = 0;
    let mut claims = 0;
    for call in calls.iter() {
        if let Operation::Write(_, event) = &call.operation {
            let event = event.as_ref();
            if matches!(
                event,
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_))
            ) {
                packets += 1;
            }
            let cs = match event {
                GenericKvLogEvent::SsDeviceLog(c) => vec![&c.0.value],
                GenericKvLogEvent::SsLog(l) => l.0.value.claims.values().collect(),
                _ => vec![],
            };
            for c in cs {
                claims += 1;
                if c.sender == *e.h.case.h.state.b.device_id()
                    && c.distribution_type == SecretDistributionType::Split
                {
                    ensure!(
                        baseline.get(&c.id.wasm_id_str()) == Some(&claim_identity(c)?),
                        "ordinary sync recomputed/published a NEW local Split generation"
                    );
                }
            }
        }
    }
    ensure!(
        packets == 0,
        "ordinary body-absent sync attempted Distribution save/replace before admission"
    );
    eprintln!(
        "ROLLOVER ordinary ALL_Distribution_attempts={packets} all_claims_checked={claims} no_new_B_Split_identity=true"
    );
    Ok(())
}
