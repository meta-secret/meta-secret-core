use super::*;

#[tokio::test]
async fn genuine_unowned_legacy_decline_keeps_normal_verified_envelope_and_real_server_behavior()
-> Result<()> {
    let case = Case::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let index = case.h.state.index_id();
    let witness = case.h.state.witness_id();
    ensure!(
        case.h.p.repo.find_one(index.clone()).await?.is_none()
            && case.h.p.repo.find_one(witness.clone()).await?.is_none(),
        "fresh unowned Legacy needs no private preparation"
    );
    let before = case.h.p.repo.snapshot().await?;
    let log = PersistentVault::from(case.h.p.clone())
        .get_vault_log_artifact(case.h.state.b.vault_name.clone())
        .await?;
    let binding = log
        .0
        .value
        .liveboard_removal_pending
        .iter()
        .find(|b| b.request_id == case.h.state.uuid)
        .context("actual A pending RemoveC")?
        .clone();
    // No checked prepare/decide and no renamed or manually seeded Guarded record.
    MetaOrchestrator {
        p_obj: case.h.p.clone(),
        user_creds: case.h.state.b.clone(),
    }
    .decline_remove_member(case.h.state.c.device_id().to_string())
    .await?;
    let body: DeviceLogObject = case
        .h
        .p
        .find_tail_event(DeviceLogDescriptor::from(case.h.state.b.user_id()))
        .await?
        .context("real legacy producer body")?;
    ensure!(
        matches!(&body.0.value,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update))
            if update.decision == RemoveMemberDecision::Declined
                && update.request == binding.request
                && update.sender.user_data == case.h.state.b.user()
                && update.target.user_data == case.h.state.c.user()
                && update.share_transfers.is_empty()),
        "actual legacy decline differs from pending request"
    );
    let registry = PersistentRemovalEnvelope {
        p_obj: case.h.p.clone(),
    }
    .load(&case.h.state.b.user_id())
    .await?
    .context("actual legacy producer registry")?;
    ensure!(
        registry
            .0
            .value
            .entries
            .iter()
            .any(|e| e.event_id == body.obj_id()
                && e.mode == RemovalEnvelopeMode::Legacy
                && e.request_id.is_none()),
        "real unowned Legacy entry missing"
    );
    ensure!(
        case.h.p.repo.find_one(index.clone()).await?.is_none()
            && case.h.p.repo.find_one(witness.clone()).await?.is_none(),
        "legacy producer allocated private preparation"
    );
    let wire = Arc::new(wire::Forward::new(&case));
    let gateway = gateway(&case, wire.clone());
    let signed = gateway
        .sign_queued_device_event(body.clone(), &case.h.state.b.user())
        .await?;
    ensure!(
        matches!(&signed.payload, SignedActionPayload::Event(GenericKvLogEvent::DeviceLog(actual))
        if *actual == body),
        "genuine unowned Legacy did not keep plain Event(DeviceLog)"
    );
    signed.verify(&case.h.state.b.user().device.keys.dsa_pk)?;
    gateway.sync(case.h.state.b.user()).await?;
    let actions = wire.device_actions.lock().unwrap().clone();
    ensure!(
        actions.len() == 1 && actions[0] == signed && wire.noops.load(Ordering::SeqCst) == 0,
        "legacy ordinary sync changed source envelope or sent Noop"
    );
    ensure!(
        case.h.state.server_repo.find_one(body.obj_id()).await? == Some(body.to_generic()),
        "real server did not persist exact legacy body"
    );
    let server = PersistentVault::from(Arc::new(PersistentObject::new(
        case.h.state.server_repo.clone(),
    )));
    let server_log = server
        .get_vault_log_artifact(case.h.state.b.vault_name.clone())
        .await?;
    let result = server_log
        .0
        .value
        .liveboard_removal_results
        .iter()
        .find(|r| r.request_id == case.h.state.uuid)
        .context("actual canonical legacy Decline")?;
    ensure!(
        result.outcome == DecisionOutcome::Declined
            && result.application_state == RemovalApplicationState::Applied
            && result.responder_id == *case.h.state.b.device_id()
            && result.source.is_none()
            && !server_log
                .0
                .value
                .liveboard_removal_pending
                .iter()
                .any(|p| p.request_id == case.h.state.uuid),
        "legacy canonical terminal result differs"
    );
    let server_members = server
        .get_vault(case.h.state.b.vault_name.clone())
        .await?
        .to_data()
        .members()
        .len();
    let local_members = PersistentVault::from(case.h.p.clone())
        .get_vault(case.h.state.b.vault_name.clone())
        .await?
        .to_data()
        .members()
        .len();
    let original_only = case
        .observations
        .live
        .lock()
        .unwrap()
        .iter()
        .all(|w| before.get(&w.packet.obj_id()) == Some(&w.packet.clone().to_generic()));
    let no_new_split = case.h.p.repo.snapshot().await?.values().all(|e|
        !matches!(e, GenericKvLogEvent::SsDeviceLog(c) if c.0.value.sender == *case.h.state.b.device_id()));
    ensure!(
        server_members == 3 && local_members == 3 && original_only && no_new_split,
        "legacy Decline changed members/material or created K1 generation"
    );
    for (id, raw) in &before {
        if matches!(
            raw,
            GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_))
        ) {
            ensure!(
                case.h.p.repo.find_one(id.clone()).await? == Some(raw.clone()),
                "legacy Decline changed original distribution bytes"
            );
        }
    }
    ensure!(
        case.h.p.repo.find_one(index).await?.is_none()
            && case.h.p.repo.find_one(witness).await?.is_none(),
        "legacy sync invented private ownership"
    );
    eprintln!(
        "C2 ordinary-registry genuineUNOWNEDLegacy actualproducer=true private_index_absent=true private_witness_absent=true verifiable_plain_Event=true actual_DeviceLog_write=1 canonical_Decline_Applied=true members3=true original_material_only=true no_new_K1=true noops=0"
    );
    Ok(())
}
