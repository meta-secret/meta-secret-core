use super::*;

#[tokio::test]
async fn actual_pending_tombstone_retires_owned_body_with_real_noop_then_explicit_next_command_progresses()
-> Result<()> {
    let (case, repo) = case().await?;
    let ordinary_before = repo.inner.get_db().await;
    let (receipt, frozen) = case.prepare(true).await?;
    let original = case.h.witness().await?;
    private_crypto(&case.h.state, &original)?;
    let origin = original
        .0
        .value
        .origin
        .clone()
        .context("actual owned N origin")?;
    let body = repo
        .inner
        .find_one(receipt.event_id.clone())
        .await?
        .context("actual owned queued body N")?
        .device_log()?;
    ensure!(
        origin.event_id == receipt.event_id
            && origin.body_commitment == producer_source_commitment(&body)?
            && original.0.value.payload.as_ref() == Some(&frozen)
            && origin.business_signature.is_none()
            && case.observations.sent.lock().unwrap().is_empty(),
        "actual owned two-Pass N must exist but remain unsent"
    );
    repo.capture_own_body(body.clone(), case.h.state.witness_id())
        .await?;
    let request = tombstone::delete_first(&case).await?;
    ensure!(
        request != frozen.update.request,
        "real tombstone did not supersede original request bytes"
    );
    let canonical_before = canonical(&case).await?;
    let writes_before = repo.packet_writes.lock().unwrap().len();
    let wire = Arc::new(wire::Forward::new(&case, repo.clone()));
    let gateway = gateway(&case, wire.clone());
    let cuts_before = wire.cuts.lock().unwrap().len();
    let sync = gateway.sync(case.h.state.b.user()).await;
    let exact_cutpoint = wire.stopped(&sync, cuts_before);
    let stored = case.h.witness().await?;
    let after = repo.inner.get_db().await;
    let ordinary_clean = checks::ordinary_clean(&after, &case.h.state.passes[0]);
    let private_clean = stored.0.value.payload.is_none();
    let no_new_material =
        checks::no_new_material(&case, &repo, (&ordinary_before, writes_before), &after);
    let technical = checks::settled(&case, &repo, &wire, &origin).await;
    let server_n = case
        .h
        .state
        .server_repo
        .find_one(origin.event_id.clone())
        .await?;
    let actual_marker = matches!(server_n, Some(GenericKvLogEvent::DeviceTransportNoop(_)));
    let canonical_same = canonical(&case).await? == canonical_before;
    eprintln!(
        "C3 Pending exact_typed_SS_cutpoint={exact_cutpoint} actual_cut_delta={} actual_Vault_reads={} actual_Noop_writes={} actual_Business_writes={} actual_server_marker={actual_marker} actual_auth_receipt_separate_tail_valid={} ordinary_scrub={ordinary_clean} private_scrub={private_clean} no_new_material={no_new_material} canonical_unchanged={canonical_same}",
        wire.cuts.lock().unwrap().len() - cuts_before,
        wire.vault_reads.load(Ordering::SeqCst),
        wire.noops.lock().unwrap().len(),
        wire.device_actions.lock().unwrap().len(),
        technical.is_ok()
    );
    checks::same_pending(&case, &request).await?;
    ensure!(
        exact_cutpoint && ordinary_clean && private_clean && no_new_material && canonical_same,
        "actual public sync-to-SS boundary did not retire stale owned N and perform mandatory cleanup without Business effects"
    );
    let technical = technical?;
    ensure!(
        actual_marker,
        "Noop was not committed to actual server slot N"
    );
    let bodies = checks::own_bodies(&case, &after);
    ensure!(bodies.len() == 1 && bodies[0].obj_id() == origin.event_id
        && !checks::action_transfer(&bodies[0].0.value, &case.h.state.passes[0])
        && bodies[0] != body
        && !after.values().any(|e| matches!(e, GenericKvLogEvent::DeviceTransportNoop(n) if n.obj_id() == origin.event_id)),
        "retirement replaced local Business with server marker or auto-appended another body");
    let server_marker = case
        .h
        .state
        .server_repo
        .find_one(origin.event_id.clone())
        .await?;

    // Only NOW a new explicit domain command is requested. No auto rollover.
    SecretDeletionAction {
        p_obj: case.h.p.clone(),
        user_creds: case.h.state.b.clone(),
    }
    .request(case.h.state.passes[1].clone())
    .await?;
    let next: DeviceLogObject = case
        .h
        .p
        .find_tail_event(DeviceLogDescriptor::from(case.h.state.b.user_id()))
        .await?
        .context("actual explicit B next command")?;
    let new_request_id = match &next.0.value {
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(r))
            if r.pass_id == case.h.state.passes[1]
                && r.sender.user_data == case.h.state.b.user() =>
        {
            r.request_id.clone()
        }
        _ => anyhow::bail!("explicit B producer did not create actual Delete p1"),
    };
    ensure!(
        next.obj_id() == origin.event_id.clone().next(),
        "explicit producer failed to advance actual N+1"
    );
    let cuts_before_next = wire.cuts.lock().unwrap().len();
    let next_sync = gateway.sync(case.h.state.b.user()).await;
    ensure!(
        wire.stopped(&next_sync, cuts_before_next),
        "explicit N+1 did not reach exact typed SS component boundary once"
    );
    let tail = wire
        .send(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
            ServerTailRequest {
                sender: case.h.state.b.user(),
                producer_reference: None,
            },
        ))))
        .await?
        .to_server_tail()?;
    let settled_again = case
        .h
        .witness()
        .await?
        .0
        .value
        .transport_noop
        .context("old technical proof survived")?;
    let (vault, log) = canonical(&case).await?;
    ensure!(
        tail.device_log_tail == Some(next.obj_id().next())
            && case.h.state.server_repo.find_one(next.obj_id()).await? == Some(next.to_generic())
            && log
                .0
                .value
                .requests
                .iter()
                .any(|r| matches!(r, VaultActionRequestEvent::DeleteSecret(r)
            if r.request_id == new_request_id && r.pass_id == case.h.state.passes[1]))
            && vault.0.value.secrets.contains(&case.h.state.passes[1])
            && settled_again.authorization == technical.authorization
            && settled_again.receipt == technical.receipt
            && case.h.state.server_repo.find_one(origin.event_id).await? == server_marker
            && wire.noops.lock().unwrap().len() == 1,
        "actual N+1 did not progress or changed old immutable Noop receipt/auth"
    );
    checks::same_pending(&case, &request).await?;
    ensure!(
        case.h.witness().await?.0.value.payload.is_none(),
        "later command resurrected retired private payload"
    );
    ensure!(
        repo.inner.find_one(bodies[0].obj_id()).await? == Some(bodies[0].clone().to_generic()),
        "later synchronization replaced the retained redacted local Business with a marker/history value"
    );
    eprintln!(
        "C3 Pending LATE actual_Nplus1_domain_command=true exact_typed_SS_cutpoint_delta1=true actual_remote_pending_Delete=true actual_fresh_tail_Nplus2=true old_auth_receipt_immutable=true SAME_Remove_UUID_pending=true no_automatic_decision=true"
    );
    Ok(())
}
