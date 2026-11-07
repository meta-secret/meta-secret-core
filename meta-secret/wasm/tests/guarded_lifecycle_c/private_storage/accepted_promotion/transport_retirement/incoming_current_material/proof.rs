use super::*;
pub(super) fn baseline_claims(
    db: &HashMap<ArtifactId, GenericKvLogEvent>,
) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    for e in db.values() {
        match e {
            GenericKvLogEvent::SsLog(l) => {
                for c in l.0.value.claims.values() {
                    out.insert(c.id.wasm_id_str(), claim_identity(c)?);
                }
            }
            GenericKvLogEvent::SsDeviceLog(l) => {
                out.insert(l.0.value.id.wasm_id_str(), claim_identity(&l.0.value)?);
            }
            _ => {}
        }
    }
    Ok(out)
}
pub(super) async fn technical(
    case: &Case,
    local: &local_repo::Repo,
    repo: &repo::Repo,
    wire: &wire::Forward,
    original: &LocalGuardedPreparationObject,
    body: &DeviceLogObject,
) -> Result<LocalGuardedPreparationObject> {
    let (stored, first_order, first_body) = {
        let boundaries = wire.boundaries.lock().unwrap();
        let first = boundaries
            .first()
            .context("actual FIRST uncut SS boundary UNREACHED")?;
        (first.snapshot.clone(), first.order, first.body.clone())
    };
    let origin = original
        .0
        .value
        .origin
        .as_ref()
        .context("real queued origin")?;
    let n = stored
        .0
        .value
        .transport_noop
        .as_ref()
        .context("actual settled transport before SS")?;
    let auth = &n.authorization;
    auth.verify(&case.h.state.b.user().device.keys.dsa_pk)?;
    ensure!(
        auth.event_id == origin.event_id
            && auth.actor_id == origin.actor_id
            && auth.vault_name == origin.vault_name
            && auth.stream == origin.event_id.fqdn.clone().id_str()
            && auth.nonce == origin.event_id.id.curr as u64
            && auth.client_entry_commitment == origin.body_commitment,
        "purpose-bound exact original authorization differs"
    );
    let receipt = n.receipt.as_ref().context("actual Noop receipt")?;
    ensure!(
        receipt.version == 1
            && receipt.event_id == origin.event_id
            && receipt.actor_id == origin.actor_id
            && receipt.vault_name == origin.vault_name
            && receipt.client_entry_commitment == origin.body_commitment
            && receipt.authorization_hash == auth.authorization_hash()?
            && receipt.record_effect == TransportNoopEffect::NotApplied
            && receipt.transport_state == TransportNoopState::NoopCommitted
            && n.confirmed_next_tail == Some(origin.event_id.clone().next()),
        "technical receipt/tail differs"
    );
    let mut expected = original.clone();
    expected.0.value.payload = None;
    expected.0.value.phase = GuardedPreparationPhase::BlockedRetiredCommand;
    expected.0.value.transport_noop = Some(n.clone());
    ensure!(
        stored == expected,
        "Noop rewrote compact origin/claim/packet/context/history or retained private FULL"
    );
    {
        let noops = wire.noops.lock().unwrap();
        let [noop] = noops.as_slice() else {
            anyhow::bail!("actual first full sync Noop count differs")
        };
        ensure!(
            noop.auth == *auth
                && noop.entry < noop.done
                && noop.done < first_order
                && noop.durable
                && noop.readback
                && noop.private_clean
                && noop.ordinary_clean
                && noop.receipt.as_ref() == Some(receipt),
            "Noop before auth-production-readback/mandatory private+ordinary purge"
        );
        let replies = wire.replies.lock().unwrap();
        let tail=replies.iter().find(|r|r.order>noop.done&&r.order<first_order&&matches!(&r.response,DataSyncResponse::ServerTailResponse(t)if t.device_log_tail==Some(origin.event_id.clone().next()))&&matches!(&r.request,SyncRequest::Read(q)if matches!(&**q,ReadSyncRequest::ServerTail(_)))).context("separate actual postreceipt Tail before FIRST SS absent")?;
        ensure!(repo.reads.lock().unwrap().iter().any(|r|r.order>tail.order&&r.order<first_order&&matches!(&r.event,Some(GenericKvLogEvent::LocalGuardedPreparation(w))if w.0.value.transport_noop.as_ref()==Some(n))),"actual outer durable confirmed-tail readback before SS absent");
    }
    let raw = case
        .h
        .state
        .server_repo
        .find_one(origin.event_id.clone())
        .await?
        .context("real server N absent")?;
    let GenericKvLogEvent::DeviceTransportNoop(marker) = raw else {
        anyhow::bail!("server N not technical marker")
    };
    marker.validate(
        &case.h.state.b.user_id(),
        &case.h.state.b.user().device.keys.dsa_pk,
    )?;
    ensure!(
        marker.0.value.authorization == *auth && marker.receipt() == *receipt,
        "server marker/receipt differs"
    );
    let mut redacted = body.clone();
    if let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(u)) = &mut redacted.0.value
    {
        u.request
            .share_transfers
            .retain(|t| t.pass_id != case.h.state.passes[0]);
        u.share_transfers
            .retain(|t| t.pass_id != case.h.state.passes[0]);
    } else {
        anyhow::bail!("real owned N kind differs")
    };
    ensure!(
        first_body == redacted.clone().to_generic(),
        "local N replaced by marker/deleted/original private transfers"
    );
    ensure!(
        local.captured_original_before_auth.load(Ordering::SeqCst)
            && !local.body_changes.lock().unwrap().is_empty()
            && local
                .body_changes
                .lock()
                .unwrap()
                .iter()
                .all(|c| c.authorization.as_ref() == Some(auth) && c.prior_auth_readback)
            && local
                .scrubs
                .lock()
                .unwrap()
                .iter()
                .any(|s| s.authorization.as_ref() == Some(auth) && s.prior_auth_readback),
        "actual owned N/private scrub preceded auth readback"
    );
    for call in repo.calls.lock().unwrap().iter() {
        let repo::Op::Write(replace, event) = &call.op else {
            continue;
        };
        let event = event.as_ref();
        let private_scrub = matches!(event,GenericKvLogEvent::LocalGuardedPreparation(w)if w.obj_id()==case.h.state.witness_id()&&w.0.value.payload.is_none());
        let owned_change = event.obj_id() == body.obj_id() && *event != body.clone().to_generic();
        if private_scrub || owned_change {
            ensure!(*replace&&repo.reads.lock().unwrap().iter().any(|r|r.order<call.order&&matches!(&r.event,Some(GenericKvLogEvent::LocalGuardedPreparation(w))if w.obj_id()==case.h.state.witness_id()&&w.0.value.transport_noop.as_ref().is_some_and(|n|n.authorization==*auth))),"actual outer production authorization readback did not precede private/own-N scrub");
        }
    }
    eprintln!(
        "C4_INCOMING actual_Noop=true original_auth_production_readback=true private_ordinary_purge_before_send=true receipt_separate_Tail_confirmed_readback_before_FIRST_SS=true exact_compact_retained_redacted_N=true"
    );
    Ok(stored)
}
pub(super) fn all_attempts(
    case: &Case,
    repo: &repo::Repo,
    wire: &wire::Forward,
    before: &HashMap<ArtifactId, GenericKvLogEvent>,
    claims: &HashMap<String, String>,
    planned: &[SsClaimId],
) -> Result<()> {
    let check = |e: &GenericKvLogEvent| -> Result<()> {
        let list = match e {
            GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(p)) => {
                ensure!(
                    p.value.claim_id.pass_id != case.h.state.passes[0]
                        && !planned.contains(&p.value.claim_id)
                        && before.get(&e.obj_id()) == Some(e),
                    "retired planned/new generation packet write or upload"
                );
                vec![]
            }
            GenericKvLogEvent::SsLog(l) => l.0.value.claims.values().collect::<Vec<_>>(),
            GenericKvLogEvent::SsDeviceLog(l) => vec![&l.0.value],
            _ => vec![],
        };
        for c in list {
            ensure!(
                !planned.contains(&c.dist_claim_id)
                    && claims.get(&c.id.wasm_id_str()) == Some(&claim_identity(c)?),
                "new/retired planned claim or generation attempted"
            );
        }
        Ok(())
    };
    let settled = wire.boundaries.lock().unwrap().first().map(|b| {
        (
            b.order,
            b.snapshot.clone(),
            b.index.clone(),
            b.registry.clone(),
        )
    });
    let mut post_ss_private_attempts = 0;
    let (mut writes, mut deletes) = (0, 0);
    let mut last = 0;
    for c in repo.calls.lock().unwrap().iter() {
        ensure!(c.order > last, "ALL method order");
        last = c.order;
        match &c.op {
            repo::Op::Write(replace, e) => {
                let e = e.as_ref();
                let _ = *replace;
                writes += 1;
                check(e)?;
                if let Some((order, compact, index, registry)) = &settled
                    && c.order > *order
                {
                    match e {
                        GenericKvLogEvent::LocalGuardedPreparation(w) => {
                            post_ss_private_attempts += 1;
                            ensure!(
                                w == compact,
                                "post-FIRSTSS preparation attempt restored FULL/mutated current or previous compact history"
                            );
                        }
                        GenericKvLogEvent::LocalGuardedPreparationIndex(_) => {
                            ensure!(e == index, "post-FIRSTSS attempted changed index/history")
                        }
                        GenericKvLogEvent::LocalRemovalEnvelope(_) => ensure!(
                            e == registry,
                            "post-FIRSTSS attempted changed origin registry/history"
                        ),
                        _ => {}
                    }
                }
            }
            repo::Op::Find(id) | repo::Op::Key(id) => {
                let _ = &id.fqdn;
            }
            repo::Op::Delete(id) | repo::Op::TryDelete(id) => {
                deletes += 1;
                let _ = &id.fqdn;
            }
            _ => {}
        }
    }
    let mut raw_sends = 0;
    let mut wire_order = 0;
    for c in wire.calls.lock().unwrap().iter() {
        ensure!(c.order > wire_order, "ALL actual wire entry order");
        wire_order = c.order;
        if let SyncRequest::Write(w) = &c.request {
            match &**w {
                WriteSyncRequest::Event(a) => {
                    raw_sends += 1;
                    match &a.payload {
                        SignedActionPayload::Event(e) => {
                            ensure!(
                                !matches!(e, GenericKvLogEvent::DeviceLog(_)),
                                "Business command/ACK escaped technical retirement"
                            );
                            check(e)?;
                        }
                        SignedActionPayload::GuardedRemovalDecision(_) => {
                            anyhow::bail!("Guarded Business sent after retirement")
                        }
                        _ => {}
                    }
                }
                WriteSyncRequest::DeviceTransportNoop(_) => {}
            }
        }
    }
    ensure!(
        case.observations.sent.lock().unwrap().is_empty(),
        "original Business falsely ACKed"
    );
    eprintln!(
        "C4_INCOMING ALL_repo_writes={writes} ALL_deletes={deletes} ALL_raw_sends={raw_sends} post_FIRSTSS_private_attempts={post_ss_private_attempts} no_transient_FULL_history_restore=true no_planned_K1_or_new_generation_attempts=true"
    );
    Ok(())
}
pub(super) async fn stable(
    case: &Case,
    repo: &repo::Repo,
    wire: &wire::Forward,
    current: &crypto::Current,
    compact: &LocalGuardedPreparationObject,
    request: &RemoveMemberRequestEvent,
) -> Result<()> {
    ensure!(
        case.h.witness().await? == *compact,
        "later full sync/GetState rewrote exact compact technical ownership"
    );
    let (first_index, first_registry, first_body) = {
        let boundaries = wire.boundaries.lock().unwrap();
        let first = boundaries.first().context("first SS snapshot")?;
        (
            first.index.clone(),
            first.registry.clone(),
            first.body.clone(),
        )
    };
    ensure!(
        repo.physical.find_one(case.h.state.index_id()).await? == Some(first_index),
        "ordinary sync changed enrolled index"
    );
    let registry = PersistentRemovalEnvelope {
        p_obj: Arc::new(PersistentObject::new(repo.physical.clone())),
    }
    .load(&case.h.state.b.user_id())
    .await?
    .context("current registry")?
    .to_generic();
    ensure!(
        registry == first_registry,
        "ordinary sync changed owned registry"
    );
    let origin = compact.0.value.origin.as_ref().unwrap();
    let db = repo.physical.get_db().await;
    let bodies = checks::own_bodies(case, &db);
    ensure!(
        bodies.len() == 1
            && bodies[0].obj_id() == origin.event_id
            && bodies[0].clone().to_generic() == first_body
            && !db
                .values()
                .any(|e| matches!(e, GenericKvLogEvent::DeviceTransportNoop(_))),
        "new own body/local marker/auto N+1 appeared"
    );
    ensure!(
        checks::ordinary_clean(&db, &case.h.state.passes[0]) && compact.0.value.payload.is_none(),
        "deleted p0 private/ordinary sensitive copies remain"
    );
    let packet = repo
        .physical
        .find_one(current.b.obj_id())
        .await?
        .context("current B K2 lost")?
        .shared_secret()?;
    ensure!(
        packet == current.b,
        "current imported K2 overwritten by retired/planned output"
    );
    checks::same_pending(case, request).await?;
    let (v, _) = canonical(case).await?;
    ensure!(
        v.0.value.secrets.contains(&current.pass)
            && v.0.value.deleted_secrets.contains(&case.h.state.passes[0])
            && !v.0.value.secrets.contains(&case.h.state.passes[0]),
        "actual p1/p0/member3 state differs"
    );
    Ok(())
}
