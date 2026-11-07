use super::*;

pub(super) async fn privacy(h: &harness::Harness) -> Result<()> {
    let actual = h.repo.witness().await?;
    let db = h.repo.physical.get_db().await;
    let private_clean = actual.0.value.payload.is_none();
    let ordinary_clean = checks::ordinary_clean(&db, &h.case.h.state.passes[0]);
    let raw = h.repo.physical.find_one(h.origin.event_id.clone()).await?;
    let redacted = matches!(&raw,Some(GenericKvLogEvent::DeviceLog(b))
        if b.obj_id()==h.body.obj_id() && b!=&h.body && !checks::action_transfer(&b.0.value,&h.case.h.state.passes[0]));
    let no_new_material = h
        .repo
        .writes
        .lock()
        .unwrap()
        .iter()
        .all(|write| match &write.event {
            GenericKvLogEvent::SsWorkflow(packet @ SsWorkflowObject::Distribution(_)) => {
                h.before.get(&packet.obj_id()) == Some(&packet.clone().to_generic())
            }
            GenericKvLogEvent::SsDeviceLog(claim) => {
                claim.0.value.sender != *h.case.h.state.b.device_id()
            }
            _ => true,
        });
    let scrub_attempts = h
        .repo
        .writes
        .lock()
        .unwrap()
        .iter()
        .filter(|w| {
            matches!(&w.event,
        GenericKvLogEvent::LocalGuardedPreparation(p) if p.0.value.payload.is_none())
        })
        .count();
    let first_scrub_order = h
        .repo
        .writes
        .lock()
        .unwrap()
        .iter()
        .filter_map(|write| {
            matches!(&write.event, GenericKvLogEvent::LocalGuardedPreparation(p)
            if p.0.value.payload.is_none())
            .then_some(write.order)
        })
        .min();
    eprintln!(
        "AUTH4 privacy physical_private_clean={private_clean} physical_ordinary_clean={ordinary_clean} actual_redacted_owned_N={redacted} actual_cleanup_write_attempts={scrub_attempts} first_cleanup_attempt_order={first_scrub_order:?} no_new_material={no_new_material}"
    );
    ensure!(
        private_clean
            && ordinary_clean
            && redacted
            && no_new_material
            && actual.0.value.origin.as_ref() == Some(&h.origin)
            && actual.0.value.claims == h.original.0.value.claims
            && actual.0.value.packets == h.original.0.value.packets,
        "authorization uncertainty retained sensitive material or lost actual owned compact identity"
    );
    checks::same_pending(&h.case, &h.request).await?;
    ensure!(
        canonical(&h.case).await? == h.canonical,
        "technical cleanup changed actual canonical decision"
    );
    Ok(())
}
pub(super) async fn settled(
    h: &harness::Harness,
    result: &Result<()>,
    previous: usize,
) -> Result<()> {
    ensure!(
        h.wire.stopped(result, previous),
        "actual public auth prefix did not reach exact typed SS boundary once"
    );
    privacy(h).await?;
    let actual = h.repo.witness().await?;
    let technical = actual
        .0
        .value
        .transport_noop
        .as_ref()
        .context("actual durable technical proof")?;
    let auth = &technical.authorization;
    auth.verify(&h.case.h.state.b.user().device.keys.dsa_pk)?;
    ensure!(
        auth.event_id == h.origin.event_id
            && auth.actor_id == h.origin.actor_id
            && auth.vault_name == h.origin.vault_name
            && auth.client_entry_commitment == h.origin.body_commitment
            && auth.stream == h.origin.event_id.fqdn.clone().id_str()
            && auth.nonce == h.origin.event_id.id.curr as u64,
        "retry changed original transport identity"
    );
    let receipt = technical
        .receipt
        .as_ref()
        .context("actual stored Noop receipt")?;
    ensure!(
        receipt.event_id == h.origin.event_id
            && receipt.actor_id == h.origin.actor_id
            && receipt.vault_name == h.origin.vault_name
            && receipt.client_entry_commitment == h.origin.body_commitment
            && receipt.authorization_hash == auth.authorization_hash()?
            && receipt.version == 1
            && receipt.transport_state == TransportNoopState::NoopCommitted
            && receipt.record_effect == TransportNoopEffect::NotApplied,
        "actual Noop receipt is not exact NotApplied proof"
    );
    let send_order = {
        let noops = h.wire.noops.lock().unwrap();
        ensure!(
            noops.len() == 1
                && noops[0].authorization == *auth
                && noops[0].actual_durable
                && noops[0].outer_readback
                && noops[0].private_clean
                && noops[0].ordinary_clean
                && noops[0].receipt.as_ref() == Some(receipt),
            "send lacked actual durable auth/OUTER production readback/purge or received proof"
        );
        noops[0].order
    };
    let cut = h.wire.cuts.lock().unwrap()[0].order;
    let next = h.origin.event_id.clone().next();
    ensure!(
        technical.confirmed_next_tail == Some(next.clone())
            && h.wire
                .tails
                .lock()
                .unwrap()
                .iter()
                .any(|tail| tail.after_receipt
                    && send_order < tail.order
                    && tail.order < cut
                    && tail.value.device_log_tail == Some(next.clone())),
        "actual separate postreceipt Tail must precede FIRST SS boundary"
    );
    {
        let durable = h.repo.durable.lock().unwrap();
        let first = durable.first().context("auth never actually durable")?;
        ensure!(
            first.order < send_order
                && durable
                    .iter()
                    .all(|d| d.authorization == first.authorization)
                && first.authorization == *auth,
            "retry replaced an EVER-durable authorization"
        );
    }
    let marker = h
        .case
        .h
        .state
        .server_repo
        .find_one(h.origin.event_id.clone())
        .await?
        .context("actual server Noop N")?;
    let GenericKvLogEvent::DeviceTransportNoop(marker) = marker else {
        anyhow::bail!("actual server slot is not technical marker")
    };
    marker.validate(
        &h.case.h.state.b.user_id(),
        &h.case.h.state.b.user().device.keys.dsa_pk,
    )?;
    ensure!(
        marker.receipt() == *receipt
            && marker.0.value.authorization == *auth
            && h.wire.actions.lock().unwrap().is_empty()
            && h.case.observations.sent.lock().unwrap().is_empty(),
        "technical proof substituted for Business ACK or uploaded another operation"
    );
    Ok(())
}
