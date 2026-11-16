use super::*;

pub(super) async fn privacy<R: KvLogEventRepo>(h: &harness::Harness<R>) -> Result<()> {
    let actual = h.repo.witness().await?;
    let db = h.case.h.p.repo.snapshot().await?;
    let origin = h
        .reference
        .0
        .value
        .origin
        .as_ref()
        .context("compact original")?;
    let body = h
        .physical
        .find_one(origin.event_id.clone())
        .await?
        .context("actual redacted owned N must remain")?;
    ensure!(
        actual.0.value.payload.is_none()
            && actual.0.value.origin == h.reference.0.value.origin
            && actual.0.value.claims == h.reference.0.value.claims
            && actual.0.value.packets == h.reference.0.value.packets
            && checks::ordinary_clean(&db, &h.case.h.state.passes[0])
            && digest(&body)? != h.body_hash
            && matches!(&body,GenericKvLogEvent::DeviceLog(b) if b.obj_id()==origin.event_id
            && !checks::action_transfer(&b.0.value,&h.case.h.state.passes[0])),
        "transport uncertainty retained private/ordinary data or lost actual redacted owned N"
    );
    ensure!(
        digest(&h.case.h.index().await?)? == h.index_hash
            && digest(
                &h.physical
                    .find_one(ArtifactId::from(LocalRemovalEnvelopeDescriptor(
                        h.case.h.state.b.user_id()
                    )))
                    .await?
                    .context("actual registry")?
            )? == h.registry_hash,
        "transport changed original index/registry"
    );
    ensure!(
        h.physical
            .find_one(h.case.h.state.witness_id().next())
            .await?
            .is_none(),
        "private transport record grew history"
    );
    {
        let writes = h.repo.writes.lock().unwrap();
        let mut scrubbed = false;
        for write in writes.iter() {
            match &write.event {
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_)) => {
                    anyhow::bail!("technical retry recreated Distribution before FIRST SS import");
                }
                GenericKvLogEvent::SsLog(log) => {
                    for claim in log.0.value.claims.values() {
                        if (claim.sender == *h.case.h.state.b.device_id()
                            && claim.distribution_type == SecretDistributionType::Split)
                            || h.reference
                                .0
                                .value
                                .claims
                                .iter()
                                .any(|owned| owned.claim.id == claim.id)
                        {
                            ensure!(
                                h.claim_identities.get(&claim.id.wasm_id_str())
                                    == Some(&claim_identity(claim)?),
                                "technical retry introduced local Split/planned generation in aggregate"
                            );
                        }
                    }
                }
                GenericKvLogEvent::SsDeviceLog(c) => ensure!(
                    c.0.value.sender != *h.case.h.state.b.device_id(),
                    "technical retry generated a new local Split claim"
                ),
                GenericKvLogEvent::DeviceLog(body)
                    if body.obj_id().fqdn == origin.event_id.fqdn =>
                {
                    ensure!(
                        body.obj_id() == origin.event_id,
                        "technical retry automatically appended another own command"
                    );
                }
                GenericKvLogEvent::LocalGuardedPreparation(w)
                    if w.obj_id() == h.case.h.state.witness_id() =>
                {
                    ensure!(
                        !scrubbed || w.0.value.payload.is_none(),
                        "private FULL resurrected after scrub"
                    );
                    scrubbed |= w.0.value.payload.is_none();
                }
                _ => {}
            }
        }
        let distribution_attempts = writes
            .iter()
            .filter(|w| {
                matches!(
                    &w.event,
                    GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_))
                )
            })
            .count();
        let aggregate_attempts = writes
            .iter()
            .filter(|w| matches!(&w.event, GenericKvLogEvent::SsLog(_)))
            .count();
        eprintln!(
            "RECEIPT_TAIL PRIVACY ALL_Distribution_attempts={distribution_attempts} ALL_SsLog_attempts={aggregate_attempts} every_local_Split_or_owned_generation_identity_checked=true"
        );
    }
    {
        let changes = h.repo.changes.lock().unwrap();
        ensure!(
            changes.iter().all(|c| c.after_auth_readback && !c.deletion),
            "actual owned N changed before auth/readback or was deleted"
        );
    }
    let (vault, log) = canonical(&h.case).await?;
    ensure!(
        digest(&(vault.clone(), log.clone()))? == h.canonical_hash
            && vault.0.value.members().len() == 3
            && vault.0.value.secrets.contains(&h.case.h.state.passes[1])
            && vault
                .0
                .value
                .deleted_secrets
                .contains(&h.case.h.state.passes[0])
            && log
                .0
                .value
                .liveboard_removal_pending
                .iter()
                .filter(|b| b.request_id == h.case.h.state.uuid)
                .count()
                == 1
            && !log
                .0
                .value
                .liveboard_removal_results
                .iter()
                .any(|r| r.request_id == h.case.h.state.uuid),
        "technical Noop changed same-UUID Pending/canonical p1/membership"
    );
    ensure!(
        h.wire.actions.lock().unwrap().is_empty()
            && h.case.observations.sent.lock().unwrap().is_empty(),
        "technical proof substituted for Business ACK/source"
    );
    Ok(())
}

pub(super) async fn identity<R: KvLogEventRepo>(
    h: &harness::Harness<R>,
) -> Result<LocalGuardedTransportNoop> {
    let actual = h.repo.witness().await?;
    let technical = actual
        .0
        .value
        .transport_noop
        .context("actual durable technical authorization")?;
    let auth = &technical.authorization;
    let origin = h
        .reference
        .0
        .value
        .origin
        .as_ref()
        .context("actual compact origin")?;
    auth.verify(&h.case.h.state.b.user().device.keys.dsa_pk)?;
    ensure!(
        auth.event_id == origin.event_id
            && auth.actor_id == origin.actor_id
            && auth.vault_name == origin.vault_name
            && auth.stream == origin.event_id.fqdn.clone().id_str()
            && auth.nonce == origin.event_id.id.curr as u64
            && auth.client_entry_commitment == origin.body_commitment,
        "Noop authorization changed exact original identity"
    );
    if let Some(old) = h.reference.0.value.transport_noop.as_ref() {
        ensure!(
            old.authorization == *auth
                && old
                    .receipt
                    .as_ref()
                    .is_none_or(|r| technical.receipt.as_ref() == Some(r)),
            "reopen changed durable auth/receipt"
        );
    }
    ensure!(
        h.repo
            .durable_auth
            .lock()
            .unwrap()
            .iter()
            .all(|a| a == auth),
        "retry replaced EVER-durable authorization"
    );
    let raw = h
        .case
        .h
        .state
        .server_repo
        .find_one(origin.event_id.clone())
        .await?
        .context("actual committed server Noop N")?;
    let GenericKvLogEvent::DeviceTransportNoop(marker) = raw else {
        anyhow::bail!("server slot N is not real technical marker")
    };
    marker.validate(
        &h.case.h.state.b.user_id(),
        &h.case.h.state.b.user().device.keys.dsa_pk,
    )?;
    let receipt = marker.receipt();
    ensure!(
        marker.0.value.authorization == *auth
            && technical.receipt.as_ref().is_none_or(|r| r == &receipt)
            && h.repo
                .durable_receipts
                .lock()
                .unwrap()
                .iter()
                .all(|r| r == &receipt),
        "durable receipt identity changed"
    );
    for call in h.wire.noops.lock().unwrap().iter() {
        ensure!(
            call.auth == *auth
                && call.durable
                && call.outer_read
                && h.repo.auth_read(auth, call.order)
                && call.private_clean
                && call.ordinary_clean
                && call.receipt.as_ref() == Some(&receipt),
            "Noop send lacked actual auth/readback/purge or exact server receipt"
        );
    }
    Ok(technical)
}

pub(super) async fn settled<R: KvLogEventRepo>(
    h: &harness::Harness<R>,
    result: &Result<()>,
    previous: usize,
) -> Result<()> {
    ensure!(
        h.wire.stopped(result, previous),
        "actual bounded public sync did not reach exact first SS boundary"
    );
    privacy(h).await?;
    let technical = identity(h).await?;
    let receipt = technical
        .receipt
        .as_ref()
        .context("actual local exact receipt")?;
    ensure!(
        receipt.version == 1
            && receipt.record_effect == TransportNoopEffect::NotApplied
            && receipt.transport_state == TransportNoopState::NoopCommitted
            && receipt.authorization_hash == technical.authorization.authorization_hash()?,
        "receipt is not exact NotApplied proof"
    );
    let next = h.repo.origin.event_id.clone().next();
    let cut = h.wire.cuts.lock().unwrap()[0].order;
    ensure!(
        technical.confirmed_next_tail == Some(next.clone()),
        "next tail confirmation absent/different"
    );
    let reads = h.repo.reads.lock().unwrap();
    let tails = h.wire.tails.lock().unwrap();
    ensure!(
        tails.iter().any(|tail| tail.returned
            && tail.receipt_durable_before
            && tail.order < cut
            && tail.value.device_log_tail == Some(next.clone())
            && reads.iter().any(|r| r.order > tail.order
                && r.order < cut
                && matches!(&r.event,GenericKvLogEvent::LocalGuardedPreparation(w)
                if w.0.value.transport_noop.as_ref()==Some(&technical)))),
        "actual separate postreceipt Tail and OUTER confirmed readback must precede FIRST SS cut"
    );
    ensure!(
        reads.iter().any(|r| r.order < cut
            && matches!(&r.event,GenericKvLogEvent::LocalGuardedPreparation(w)
        if w.0.value.transport_noop.as_ref().and_then(|n|n.receipt.as_ref())==Some(receipt))),
        "local durable receipt lacks actual production observation"
    );
    Ok(())
}

pub(super) async fn interrupted<R: KvLogEventRepo>(
    h: &harness::Harness<R>,
    mode: Mode,
    result: &Result<()>,
) -> Result<()> {
    privacy(h).await?;
    identity(h).await?;
    ensure!(
        result.is_err() && h.wire.cuts.lock().unwrap().is_empty(),
        "uncertain receipt/tail falsely crossed SS settlement boundary"
    );
    if matches!(mode, Mode::ServerReplyLoss | Mode::TailLoss) {
        ensure!(
            result
                .as_ref()
                .err()
                .is_some_and(|e| e.is::<wire::ResponseLost>()),
            "original actual transport error was not preserved"
        );
    } else {
        ensure!(
            result
                .as_ref()
                .err()
                .is_some_and(|e| e.to_string() == "GuardedPreparationUnavailable"),
            "private receipt error was not normalized"
        );
    }
    Ok(())
}
