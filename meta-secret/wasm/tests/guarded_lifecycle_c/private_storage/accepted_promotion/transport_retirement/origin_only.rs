use super::*;

#[tokio::test]
async fn genuine_prebody_before_or_drop_origin_is_scrubbed_but_never_noop_eligible_or_auto_queued()
-> Result<()> {
    let mut results = vec![];
    for fault in [local_repo::Fault::Before, local_repo::Fault::Drop] {
        results.push(run(fault).await);
    }
    for result in results {
        result?;
    }
    Ok(())
}
async fn run(fault: local_repo::Fault) -> Result<()> {
    let (case, repo) = case().await?;
    let ordinary_before = repo.inner.get_db().await;
    repo.arm(case.h.state.b.user_id(), case.h.state.uuid.clone(), fault);
    let prepared = case.prepare(true).await;
    let actual_candidate = repo
        .attempted_body
        .lock()
        .unwrap()
        .clone()
        .context("actual producer body save was reached")?;
    let original = case.h.witness().await?;
    private_crypto(&case.h.state, &original)?;
    let origin = original
        .0
        .value
        .origin
        .clone()
        .context("actual prebody durable origin")?;
    ensure!(
        prepared.is_err()
            && repo.hits.load(Ordering::SeqCst) == 1
            && repo.origin_before_body.lock().unwrap().as_ref() == Some(&original)
            && repo
                .inner
                .find_one(origin.event_id.clone())
                .await?
                .is_none()
            && actual_candidate.obj_id() == origin.event_id
            && producer_source_commitment(&actual_candidate)? == origin.body_commitment
            && original
                .0
                .value
                .payload
                .as_ref()
                .is_some_and(|p| p.passes.len() == 2)
            && original.0.value.transport_noop.is_none()
            && case
                .h
                .state
                .server_repo
                .find_one(origin.event_id.clone())
                .await?
                .is_none(),
        "prebody setup must be real producer Err with prior exact origin/registry readbacks and NEVER-durable body"
    );
    eprintln!(
        "C3 OriginOnly {fault:?} real_body_save_fault_hits=1 actual_origin_registry_readbacks_before_body=true producer_Err_not_queued=true actual_local_and_server_body_absent=true FULL_twoPass=true"
    );
    let request = tombstone::delete_first(&case).await?;
    let writes_before = repo.packet_writes.lock().unwrap().len();
    let wire = Arc::new(wire::Forward::new(&case, repo.clone()));
    let cuts_before = wire.cuts.lock().unwrap().len();
    let sync = gateway(&case, wire.clone())
        .sync(case.h.state.b.user())
        .await;
    let exact_cutpoint = wire.stopped(&sync, cuts_before);
    let stored = case.h.witness().await?;
    let after = repo.inner.get_db().await;
    let private_clean = stored.0.value.payload.is_none();
    let ordinary_clean = checks::ordinary_clean(&after, &case.h.state.passes[0]);
    let body_absent = repo
        .inner
        .find_one(origin.event_id.clone())
        .await?
        .is_none();
    let server_empty = case
        .h
        .state
        .server_repo
        .find_one(origin.event_id.clone())
        .await?
        .is_none();
    let no_auth = stored.0.value.transport_noop.is_none();
    let no_noop = wire.noops.lock().unwrap().is_empty();
    let no_business = wire.device_actions.lock().unwrap().is_empty();
    let no_autoappend =
        checks::own_bodies(&case, &after).is_empty() && repo.hits.load(Ordering::SeqCst) == 1;
    let no_new_material =
        checks::no_new_material(&case, &repo, (&ordinary_before, writes_before), &after);
    eprintln!(
        "C3 OriginOnly {fault:?} exact_typed_SS_cutpoint={exact_cutpoint} actual_cut_delta={} actual_Vault_reads={} private_scrub={private_clean} ordinary_scrub={ordinary_clean} no_auth={no_auth} no_Noop={no_noop} no_Business={no_business} body_absent={body_absent} server_slot_empty={server_empty} no_autoappend={no_autoappend} no_new_material={no_new_material}",
        wire.cuts.lock().unwrap().len() - cuts_before,
        wire.vault_reads.load(Ordering::SeqCst)
    );
    checks::same_pending(&case, &request).await?;
    ensure!(
        exact_cutpoint
            && private_clean
            && ordinary_clean
            && no_auth
            && no_noop
            && no_business
            && body_absent
            && server_empty
            && no_autoappend
            && no_new_material
            && stored.0.value.origin.as_ref() == Some(&origin),
        "origin-only actual prebody uncertainty must scrub without manufacturing queued command/Noop eligibility"
    );
    Ok(())
}
