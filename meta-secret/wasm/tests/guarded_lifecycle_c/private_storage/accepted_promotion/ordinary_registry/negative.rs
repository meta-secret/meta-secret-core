use super::*;

#[derive(Clone, Copy, Debug)]
enum Corruption {
    Legacy,
    MissingEntry,
    WrongUuid,
}

#[tokio::test]
async fn actual_indexed_guarded_source_cannot_downgrade_or_lose_registry_binding_in_public_signer_and_sync()
-> Result<()> {
    let mut results = vec![];
    for variant in [
        Corruption::Legacy,
        Corruption::MissingEntry,
        Corruption::WrongUuid,
    ] {
        results.push(run(variant).await);
    }
    // Both public paths for all THREE fresh cases execute before any RED escapes.
    for result in results {
        result?;
    }
    Ok(())
}

async fn run(variant: Corruption) -> Result<()> {
    let case = Case::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let (receipt, frozen) = case.prepare(true).await?;
    let witness = case.h.witness().await?;
    private_crypto(&case.h.state, &witness)?;
    ensure!(
        witness.0.value.payload.as_ref() == Some(&frozen),
        "actual FULL source required"
    );
    let body: DeviceLogObject = case
        .h
        .p
        .repo
        .find_one(receipt.event_id.clone())
        .await?
        .context("actual checked B source body")?
        .device_log()?;
    let registry_store = PersistentRemovalEnvelope {
        p_obj: case.h.p.clone(),
    };
    let mut registry = registry_store
        .load(&case.h.state.b.user_id())
        .await?
        .context("actual healthy registry")?;
    ensure!(
        registry.0.value.first_managed_removal_nonce > 0,
        "positive managed boundary required"
    );
    let entry = registry
        .0
        .value
        .entries
        .iter_mut()
        .find(|entry| entry.event_id == receipt.event_id)
        .context("actual exact Guarded entry")?;
    ensure!(
        entry.mode == RemovalEnvelopeMode::Guarded
            && entry.request_id.as_deref() == Some(&case.h.state.uuid),
        "initial actual ownership differs"
    );
    match variant {
        Corruption::Legacy => {
            entry.mode = RemovalEnvelopeMode::Legacy;
            entry.request_id = None;
        }
        Corruption::MissingEntry => registry
            .0
            .value
            .entries
            .retain(|e| e.event_id != receipt.event_id),
        Corruption::WrongUuid => {
            let other = "00000000-0000-4000-8000-000000000000";
            ensure!(
                is_liveboard_request_id(other) && other != case.h.state.uuid,
                "distinct valid UUID required"
            );
            entry.request_id = Some(other.into());
        }
    }
    // This ONE local metadata record is the only corrupted object.
    case.h.p.repo.replace(registry.clone()).await?;
    ensure!(
        registry_store.load(&case.h.state.b.user_id()).await? == Some(registry),
        "corruption must pass structural registry validation"
    );
    ensure!(
        case.h.witness().await? == witness,
        "corruption changed private source"
    );
    let local = case.h.p.repo.snapshot().await?;
    let server = case.h.state.server_repo.get_db().await;
    let wire = Arc::new(wire::Forward::new(&case));
    let gateway = gateway(&case, wire.clone());

    let signed = gateway
        .sign_queued_device_event(body.clone(), &case.h.state.b.user())
        .await;
    let signer_rejected = signed.is_err();
    let signer_local_same = case.h.p.repo.snapshot().await? == local;
    let signer_server_same = case.h.state.server_repo.get_db().await == server;
    let signer_private_same = case.h.witness().await? == witness;
    let signer_body_same =
        case.h.p.repo.find_one(receipt.event_id.clone()).await? == Some(body.clone().to_generic());
    let signer_writes = wire.writes.load(Ordering::SeqCst);
    let signer_returned_legacy = matches!(
        &signed,
        Ok(SignedAction {
            payload: SignedActionPayload::Event(GenericKvLogEvent::DeviceLog(_)),
            ..
        })
    );
    eprintln!(
        "C2 ordinary-registry {variant:?} PUBLICsigner rejected={signer_rejected} returned_legacy={signer_returned_legacy} local_same={signer_local_same} server_same={signer_server_same} private_same={signer_private_same} body_same={signer_body_same} all_write_attempts={signer_writes}"
    );

    // Never propagate the signer assertion before exercising ordinary sync too.
    let sync = gateway.sync(case.h.state.b.user()).await;
    let sync_rejected = sync.is_err();
    let local_same = case.h.p.repo.snapshot().await? == local;
    let server_same = case.h.state.server_repo.get_db().await == server;
    let private_same = case.h.witness().await? == witness;
    let body_same = case.h.p.repo.find_one(receipt.event_id).await? == Some(body.to_generic());
    let all_writes = wire.writes.load(Ordering::SeqCst);
    let actions = wire.device_actions.lock().unwrap().clone();
    let device_writes = actions.len();
    let legacy_writes = actions
        .iter()
        .filter(|a| {
            matches!(
                a.payload,
                SignedActionPayload::Event(GenericKvLogEvent::DeviceLog(_))
            )
        })
        .count();
    let noops = wire.noops.load(Ordering::SeqCst);
    let material_writes = case.observations.live.lock().unwrap().len();
    let claim_writes = case.observations.claim_writes.lock().unwrap().len();
    private_crypto(&case.h.state, &case.h.witness().await?)?;
    eprintln!(
        "C2 ordinary-registry {variant:?} ORDINARYsync rejected={sync_rejected} local_same={local_same} server_same={server_same} private_same={private_same} body_same={body_same} ALLDeviceLog_write_attempts={device_writes} plainLegacy_attempts={legacy_writes} all_writes={all_writes} noops={noops} material_writes={material_writes} claim_writes={claim_writes}"
    );
    ensure!(
        signer_rejected
            && signer_local_same
            && signer_server_same
            && signer_private_same
            && signer_body_same
            && signer_writes == 0,
        "{variant:?}: public signer admitted/reclassified or mutated indexed Guarded source"
    );
    ensure!(
        sync_rejected
            && local_same
            && server_same
            && private_same
            && body_same
            && all_writes == 0
            && device_writes == 0
            && noops == 0
            && material_writes == 0
            && claim_writes == 0,
        "{variant:?}: ordinary sync sent/reclassified/mutated indexed Guarded source"
    );
    Ok(())
}
