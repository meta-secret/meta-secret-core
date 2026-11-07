use super::*;

pub(super) async fn settled(
    h: &harness::Harness,
    original: &LocalGuardedPreparationObject,
) -> Result<()> {
    let w = h.case.h.witness().await?;
    let origin = original
        .0
        .value
        .origin
        .as_ref()
        .context("actual original N")?;
    let t =
        w.0.value
            .transport_noop
            .as_ref()
            .context("actual Noop technical")?;
    t.authorization
        .verify(&h.case.h.state.b.user().device.keys.dsa_pk)?;
    let actual_receipt = t.receipt.as_ref().context("actual receipt")?;
    ensure!(
        t.authorization.event_id == origin.event_id
            && t.authorization.actor_id == origin.actor_id
            && t.authorization.vault_name == origin.vault_name
            && t.authorization.stream == origin.event_id.fqdn.clone().id_str()
            && t.authorization.nonce == origin.event_id.id.curr as u64
            && t.authorization.client_entry_commitment == origin.body_commitment
            && actual_receipt.version == 1
            && actual_receipt.event_id == origin.event_id
            && actual_receipt.actor_id == origin.actor_id
            && actual_receipt.vault_name == origin.vault_name
            && actual_receipt.client_entry_commitment == origin.body_commitment
            && actual_receipt.authorization_hash == t.authorization.authorization_hash()?
            && actual_receipt.transport_state == TransportNoopState::NoopCommitted,
        "exact original transport binding differs"
    );
    let mut expected = original.clone();
    expected.0.value.payload = None;
    expected.0.value.phase = GuardedPreparationPhase::BlockedRetiredCommand;
    expected.0.value.transport_noop = Some(t.clone());
    ensure!(
        w == expected,
        "retirement changed compact origin/context/history/ownership beyond allowed scrub and technical proof"
    );
    {
        let noops = h.wire.noops.lock().unwrap();
        ensure!(
            noops.len() == 1
                && noops[0].authorization == t.authorization
                && noops[0].private_clean
                && noops[0].ordinary_clean
                && noops[0].auth_readback,
            "actual original auth readback/private+ordinary purge did not precede real Noop send"
        );
        ensure!(
            w.0.value.payload.is_none()
                && w.0.value.origin.as_ref() == Some(origin)
                && w.0.value.packets == original.0.value.packets
                && !w.0.value.packets.is_empty()
                && w.0.value.claims == original.0.value.claims
                && t.receipt
                    .as_ref()
                    .is_some_and(|r| r.record_effect == TransportNoopEffect::NotApplied)
                && t.confirmed_next_tail == Some(origin.event_id.clone().next()),
            "packet-bearing retired N compact/proof differs"
        );
        let replies = h.wire.replies.lock().unwrap();
        let noop = replies.iter().find(|r| matches!(&r.response, DataSyncResponse::DeviceTransportNoop(n) if Some(n) == t.receipt.as_ref()))
            .context("real Noop receipt reply")?;
        ensure!(
            noops[0].order < noop.order,
            "actual Noop reply preceded send"
        );
        let tail = replies.iter().find(|r| r.order > noop.order
            && matches!(&r.request, SyncRequest::Read(q) if matches!(&**q, ReadSyncRequest::ServerTail(_)))
            && matches!(&r.response, DataSyncResponse::ServerTailResponse(x) if x.device_log_tail == Some(origin.event_id.clone().next())))
            .context("real separate postreceipt fresh Tail")?;
        let ss = h.wire.calls.lock().unwrap().iter().find(|c|
            matches!(&c.request, SyncRequest::Read(q) if matches!(&**q, ReadSyncRequest::SsRequest(_)))).map(|c| c.order)
            .context("FIRST uncut SS request")?;
        ensure!(h.repo.reads.lock().unwrap().iter().any(|r| r.order > tail.order && r.order < ss
            && matches!(&r.event, Some(GenericKvLogEvent::LocalGuardedPreparation(x)) if x.0.value.transport_noop.as_ref() == Some(t))),
            "actual confirmation readback not before FIRST SS");
    }
    let marker = h
        .case
        .h
        .state
        .server_repo
        .find_one(origin.event_id.clone())
        .await?
        .context("actual Noop marker")?;
    let GenericKvLogEvent::DeviceTransportNoop(marker) = marker else {
        anyhow::bail!("actual server N is not Noop marker");
    };
    marker.validate(
        &h.case.h.state.b.user_id(),
        &h.case.h.state.b.user().device.keys.dsa_pk,
    )?;
    ensure!(
        marker.0.value.authorization == t.authorization
            && marker.receipt() == *actual_receipt
            && checks::ordinary_clean(&h.local.inner.get_db().await, &h.case.h.state.passes[0]),
        "actual validated server marker/receipt or p0 purge differs"
    );
    eprintln!(
        "OWNED_LIFETIME packetbearing_N_retired=true actual_auth_receipt_separateTail_before_FIRST_SS=true full_sync_ok=true"
    );
    Ok(())
}

pub(super) async fn published(
    h: &harness::Harness,
    frozen: &GuardedPreparationPayload,
) -> Result<()> {
    let w = h.case.h.witness().await?;
    ensure!(
        w.0.value.phase == GuardedPreparationPhase::AcceptedPublished
            && w.0.value.payload.is_none()
            && w.0.value.transport_noop.is_none(),
        "actual Applied publication compact not reached"
    );
    let mut ids = vec![];
    for pass in &frozen.passes {
        let own =
            w.0.value
                .claims
                .iter()
                .find(|c| c.pass_id == pass.pass_id)
                .context("actual claim ownership")?;
        let id = own
            .ss_device_event_id
            .clone()
            .context("actual sequential own claim ID")?;
        ids.push(id.clone());
        let raw = h
            .local
            .inner
            .find_one(id.clone())
            .await?
            .context("durable actual own claim")?;
        ensure!(
            matches!(&raw, GenericKvLogEvent::SsDeviceLog(c) if c.0.value == pass.claim)
                && h.case.h.state.server_repo.find_one(id).await? == Some(raw),
            "actual sequential claim not admitted"
        );
        for packet in &pass.distributions {
            let data = packet.clone().to_distribution_data()?;
            let receiver = data
                .secret_message
                .cipher_text()
                .channel
                .receiver()
                .to_device_id();
            if receiver == *h.case.h.state.b.device_id() {
                continue;
            }
            let deleted_after_read = {
                let replies = h.wire.replies.lock().unwrap();
                let ack = replies
                    .iter()
                    .find(|r| {
                        signed_event(&r.request).as_ref() == Some(&packet.clone().to_generic())
                            && r.response.clone().ensure_success().is_ok()
                    })
                    .context("real packet ACK")?;
                let read = h.repo.reads.lock().unwrap().iter().find(|r| r.order > ack.order
                    && matches!(&r.event, Some(GenericKvLogEvent::LocalGuardedPreparation(x)) if x.0.value.packets.iter()
                        .any(|p| p.event_id == packet.obj_id() && p.handed_off && p.body_commitment == digest(packet).unwrap())))
                    .map(|r| r.order).context("actual ACK handoff compact outer readback")?;
                h.repo.calls.lock().unwrap().iter().any(|c| c.order > read
                    && matches!(&c.op, repo::Op::Delete(id) | repo::Op::TryDelete(id) if *id == packet.obj_id()))
            };
            ensure!(
                deleted_after_read && h.local.inner.find_one(packet.obj_id()).await?.is_none(),
                "peer raw not deleted AFTER actual handoff readback"
            );
        }
    }
    ids.sort_by_key(|id| id.id.curr);
    ensure!(
        !ids.is_empty() && ids.windows(2).all(|p| p[1] == p[0].clone().next()),
        "sequential owned claim metadata not contiguous"
    );
    eprintln!(
        "OWNED_LIFETIME published_own_claims={} actual_contiguous_metadata_admission=true current_Pending_raw_ACK_handoff_readback_peer_delete=true",
        ids.len()
    );
    Ok(())
}
pub(super) fn signed_event(request: &SyncRequest) -> Option<GenericKvLogEvent> {
    match request {
        SyncRequest::Write(w) => match &**w {
            WriteSyncRequest::Event(s) => match &s.payload {
                SignedActionPayload::Event(e) => Some(e.clone()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

pub(super) async fn member2_twice(h: &harness::Harness) -> Result<LocalGuardedPreparationObject> {
    let before = h.case.h.witness().await?;
    ensure!(
        before.0.value.previous_attempts.len() == 1
            && !before.0.value.previous_attempts[0].packets.is_empty()
            && before.0.value.previous_attempts[0].transport_noop.is_some(),
        "packetbearing previous N absent"
    );
    let baseline = claim_baseline(&h.local.inner.get_db().await)?;
    let cursor = h.repo.calls.lock().unwrap().len();
    let wire_cursor = h.wire.calls.lock().unwrap().len();
    let service = h.service();
    for _ in 0..2 {
        let state = service
            .get_app_state()
            .await
            .context("SEMANTIC real Member2 owned finalizer GetState")?;
        ensure!(
            matches!(state, ApplicationState::Vault(VaultFullInfo::Member(m)) if m.member.vault.members().len() == 2
            && m.member.member.user_data == h.case.h.state.b.user()),
            "actual Member2 state projection differs"
        );
        ensure!(
            h.case.h.witness().await? == before,
            "Member2 GetState changed current/previous compact"
        );
    }
    no_new_material(h, cursor, wire_cursor, true, &baseline)?;
    ensure!(h.repo.calls.lock().unwrap()[cursor..].iter().all(|c| !matches!(&c.op, repo::Op::Write(_, event) if matches!(event.as_ref(), GenericKvLogEvent::LocalGuardedPreparation(w) if w != &before))), "Member2 transient private FULL/history/proof mutation");
    eprintln!(
        "OWNED_LIFETIME actual_same_service_Member2_GetState_TWICE=true exact_current_previous=true no_new_outputs=true"
    );
    Ok(before)
}
pub(super) fn no_new_material(
    h: &harness::Harness,
    repo_cursor: usize,
    wire_cursor: usize,
    all: bool,
    baseline: &HashMap<String, String>,
) -> Result<()> {
    for call in &h.repo.calls.lock().unwrap()[repo_cursor..] {
        if let repo::Op::Write(_, event) = &call.op {
            for claim in claims(event) {
                if claim.sender == *h.case.h.state.b.device_id()
                    && claim.distribution_type == SecretDistributionType::Split
                {
                    ensure!(
                        baseline.get(&claim.id.wasm_id_str()) == Some(&claim_identity(claim)?),
                        "ALL aggregate/own claim attempts introduced a new local generation"
                    );
                }
            }
        }
    }
    let peers = [&h.case.h.state.a, &h.case.h.state.c];
    let scoped = |event: &GenericKvLogEvent| -> bool {
        let GenericKvLogEvent::SsWorkflow(p @ SsWorkflowObject::Distribution(_)) = event else {
            return false;
        };
        if all {
            return true;
        }
        h.case
            .h
            .state
            .passes
            .iter()
            .any(|pass| peers.iter().any(|who| packet_id(pass, who) == p.obj_id()))
            || p.clone().to_distribution_data().is_ok_and(|d| {
                d.vault_name == h.case.h.state.b.vault_name
                    && h.case.h.state.passes.contains(&d.claim_id.pass_id)
                    && peers.iter().any(|who| {
                        d.secret_message
                            .cipher_text()
                            .channel
                            .receiver()
                            .to_device_id()
                            == *who.device_id()
                    })
            })
    };
    ensure!(h.repo.calls.lock().unwrap()[repo_cursor..].iter().all(|c|
        !matches!(&c.op, repo::Op::Write(_, e) if scoped(e) || matches!(e.as_ref(), GenericKvLogEvent::SsDeviceLog(_)))),
        "ALL matching packet/claim write attempts after delivery/GetState");
    ensure!(
        h.wire.calls.lock().unwrap()[wire_cursor..]
            .iter()
            .all(|c| proof::signed_event(&c.request)
                .is_none_or(|e| !scoped(&e) && !matches!(e, GenericKvLogEvent::SsDeviceLog(_)))),
        "ALL matching packet/claim upload attempts after delivery/GetState"
    );
    Ok(())
}

fn claims(event: &GenericKvLogEvent) -> Vec<&SsClaim> {
    match event {
        GenericKvLogEvent::SsDeviceLog(c) => vec![&c.0.value],
        GenericKvLogEvent::SsLog(l) => l.0.value.claims.values().collect(),
        _ => vec![],
    }
}
fn claim_identity(c: &SsClaim) -> Result<String> {
    digest(&(
        &c.id,
        &c.dist_claim_id,
        &c.vault_name,
        &c.sender,
        &c.distribution_type,
        &c.receivers,
    ))
}
pub(super) fn claim_baseline(
    db: &HashMap<ArtifactId, GenericKvLogEvent>,
) -> Result<HashMap<String, String>> {
    let mut result = HashMap::new();
    for event in db.values() {
        for c in claims(event) {
            result.insert(c.id.wasm_id_str(), claim_identity(c)?);
        }
    }
    Ok(result)
}
