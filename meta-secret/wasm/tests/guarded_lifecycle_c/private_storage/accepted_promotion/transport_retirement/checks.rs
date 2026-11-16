use super::*;
use std::collections::HashMap;

fn request_transfer(request: &VaultActionRequestEvent, pass: &MetaPasswordId) -> bool {
    matches!(request, VaultActionRequestEvent::RemoveMember(r)
        if r.share_transfers.iter().any(|t| &t.pass_id == pass))
}
pub(super) fn action_transfer(action: &VaultActionEvent, pass: &MetaPasswordId) -> bool {
    match action {
        VaultActionEvent::Request(r) => request_transfer(r, pass),
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(u)) => {
            u.request.share_transfers.iter().any(|t| &t.pass_id == pass)
                || u.share_transfers.iter().any(|t| &t.pass_id == pass)
        }
        _ => false,
    }
}
/// Ciphertext-bearing ordinary records only; tombstone/result IDs are retained metadata.
pub(super) fn ordinary_clean(
    db: &HashMap<ArtifactId, GenericKvLogEvent>,
    pass: &MetaPasswordId,
) -> bool {
    db.values().all(|event| match event {
        GenericKvLogEvent::DeviceLog(body) => !action_transfer(&body.0.value, pass),
        GenericKvLogEvent::VaultLog(log) => {
            log.0
                .value
                .requests
                .iter()
                .all(|r| !request_transfer(r, pass))
                && log
                    .0
                    .value
                    .updates
                    .iter()
                    .all(|u| !action_transfer(&VaultActionEvent::Update(u.clone()), pass))
                && log
                    .0
                    .value
                    .liveboard_removal_pending
                    .iter()
                    .all(|b| b.request.share_transfers.iter().all(|t| &t.pass_id != pass))
        }
        GenericKvLogEvent::SsWorkflow(
            SsWorkflowObject::Distribution(p) | SsWorkflowObject::Recovery(p),
        ) => &p.value.claim_id.pass_id != pass,
        _ => true,
    })
}
pub(super) fn own_bodies(
    case: &Case,
    db: &HashMap<ArtifactId, GenericKvLogEvent>,
) -> Vec<DeviceLogObject> {
    let stream = ArtifactId::from(DeviceLogDescriptor::from(case.h.state.b.user_id())).fqdn;
    db.values()
        .filter_map(|event| match event {
            GenericKvLogEvent::DeviceLog(body) if body.obj_id().fqdn == stream => {
                Some(body.clone())
            }
            _ => None,
        })
        .collect()
}
pub(super) fn no_new_material(
    case: &Case,
    repo: &local_repo::Repo,
    baseline: (&HashMap<ArtifactId, GenericKvLogEvent>, usize),
    after: &HashMap<ArtifactId, GenericKvLogEvent>,
) -> bool {
    let (before, writes_before) = baseline;
    repo.packet_writes.lock().unwrap()[writes_before..]
        .iter()
        .all(|p| before.get(&p.obj_id()) == Some(&p.clone().to_generic()))
        && after.values().all(|event| {
            !matches!(event, GenericKvLogEvent::SsDeviceLog(claim)
            if claim.0.value.sender == *case.h.state.b.device_id())
        })
}
pub(super) async fn same_pending(case: &Case, request: &RemoveMemberRequestEvent) -> Result<()> {
    let (vault, log) = canonical(case).await?;
    let matches: Vec<_> = log
        .0
        .value
        .liveboard_removal_pending
        .iter()
        .filter(|b| b.request_id == case.h.state.uuid)
        .collect();
    ensure!(
        matches.len() == 1
            && matches[0].request == *request
            && !log
                .0
                .value
                .liveboard_removal_results
                .iter()
                .any(|r| r.request_id == case.h.state.uuid)
            && vault.0.value.members().len() == 3,
        "Noop decided/rejected/replaced the still-Pending removal"
    );
    Ok(())
}
pub(super) async fn settled(
    case: &Case,
    repo: &local_repo::Repo,
    wire: &wire::Forward,
    origin: &GuardedCommandOrigin,
) -> Result<LocalGuardedTransportNoop> {
    let actual = case.h.witness().await?;
    ensure!(
        actual.0.value.origin.as_ref() == Some(origin) && actual.0.value.payload.is_none(),
        "retirement lost immutable original origin or retained sensitive private payload"
    );
    let technical = actual
        .0
        .value
        .transport_noop
        .clone()
        .context("actual durable Noop auth+receipt+tail")?;
    let auth = &technical.authorization;
    auth.verify(&case.h.state.b.user().device.keys.dsa_pk)?;
    ensure!(
        auth.event_id == origin.event_id
            && auth.actor_id == origin.actor_id
            && auth.vault_name == origin.vault_name
            && auth.stream == origin.event_id.fqdn.clone().id_str()
            && auth.nonce == origin.event_id.id.curr as u64
            && auth.client_entry_commitment == origin.body_commitment,
        "Noop auth differs from original actual command context"
    );
    let receipt = technical
        .receipt
        .as_ref()
        .context("actual stored receipt")?;
    ensure!(
        receipt.version == 1
            && receipt.event_id == origin.event_id
            && receipt.actor_id == origin.actor_id
            && receipt.vault_name == origin.vault_name
            && receipt.client_entry_commitment == origin.body_commitment
            && receipt.authorization_hash == auth.authorization_hash()?
            && receipt.transport_state == TransportNoopState::NoopCommitted
            && receipt.record_effect == TransportNoopEffect::NotApplied,
        "actual receipt is not exact purpose-bound NotApplied proof"
    );
    let noop_order = {
        let calls = wire.noops.lock().unwrap();
        ensure!(
            calls.len() == 1
                && calls[0].authorization == *auth
                && calls[0].actual_auth_durable_before
                && calls[0].production_auth_readback_before
                && calls[0].private_scrub_before
                && calls[0].ordinary_scrub_before
                && calls[0].receipt.as_ref() == Some(receipt),
            "Noop send did not follow actual auth readback and mandatory purge"
        );
        calls[0].order
    };
    let first_cut_order = wire
        .cuts
        .lock()
        .unwrap()
        .first()
        .map(|cut| cut.order)
        .context("actual first SS component boundary")?;
    {
        let tails = wire.tails.lock().unwrap();
        let next = origin.event_id.clone().next();
        ensure!(
            tails.iter().any(|tail| tail.order > noop_order
                && tail.order < first_cut_order
                && tail.after_actual_receipt
                && tail.actual.device_log_tail == Some(next.clone()))
                && technical.confirmed_next_tail == Some(next),
            "next tail was not a separate actual postreceipt ServerTail response before first SS boundary"
        );
    }
    let raw = case
        .h
        .state
        .server_repo
        .find_one(origin.event_id.clone())
        .await?
        .context("actual server N")?;
    let GenericKvLogEvent::DeviceTransportNoop(marker) = raw else {
        anyhow::bail!("actual server N is not technical marker")
    };
    marker.validate(
        &case.h.state.b.user_id(),
        &case.h.state.b.user().device.keys.dsa_pk,
    )?;
    ensure!(
        marker.0.value.authorization == *auth
            && marker.receipt() == *receipt
            && wire.device_actions.lock().unwrap().is_empty()
            && case.observations.sent.lock().unwrap().is_empty(),
        "technical proof was substituted for Business/raw ACK"
    );
    let scrubs = repo.scrubs.lock().unwrap();
    ensure!(
        scrubs
            .iter()
            .any(|s| s.authorization.as_ref() == Some(auth) && s.prior_auth_readback),
        "sensitive FULL stage was scrubbed before exact authorization readback"
    );
    let body_changes = repo.body_changes.lock().unwrap();
    ensure!(
        repo.captured_original_before_auth.load(Ordering::SeqCst)
            && !body_changes.is_empty()
            && body_changes
                .iter()
                .all(|change| change.authorization.as_ref() == Some(auth)
                    && change.prior_auth_readback),
        "actual owned N was replaced/deleted before durable exact auth and production readback"
    );
    eprintln!(
        "C3 ownN actual_original_capture_before_auth=true observed_exact_ID_changes={} observed_deletes={} all_after_durable_auth_readback=true",
        body_changes.len(),
        body_changes.iter().filter(|c| c.deletion).count()
    );
    Ok(technical)
}
