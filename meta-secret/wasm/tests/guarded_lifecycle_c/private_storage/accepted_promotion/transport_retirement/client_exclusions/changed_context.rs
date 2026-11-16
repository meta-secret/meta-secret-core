use super::*;

#[tokio::test]
async fn client_exclusions_actual_other_request_winner_retires_losing_owned_slot() -> Result<()> {
    let mut phase = "setup_initial";
    let result: Result<()> = async {
        let (data, ap, bp) = fixture::initial().await?;
        let (b_uuid, b_request) = data.request(&bp, &data.c).await?;
        phase = "setup_first_admitted";
        let (a_uuid, a_request) = data.request(&ap, &data.c).await?;
        ensure!(a_uuid != b_uuid, "DEPENDENCY distinct requesters did not receive distinct actual UUIDs");
        eprintln!("CLIENT_EXCLUSIONS changed_context B_first_admitted=true A_second_admitted=true distinct_UUIDs=true");
        bp.sync().await?;
        let queued = bp.gateway.decide_remove_member(CanonicalDecisionInput { request_id: a_uuid.clone(), accepted: true }).await?;
        let baseline = Baseline::capture(&bp, &a_uuid).await?;
        ensure!(queued.event_id == b_request.obj_id().next() && baseline.origin.event_id == queued.event_id
            && baseline.witness.0.value.payload.as_ref().is_some_and(|p| p.passes.len() == 2)
            && data.server_repo.find_one(queued.event_id.clone()).await?.is_none(),
            "DEPENDENCY unsent checked B:N must follow actual B:M prefix");
        phase = "setup_losing_N_queued";
        eprintln!("CLIENT_EXCLUSIONS changed_context real_unsent_N=M_plus1=true FULL2=true server_N_empty=true");
        let winner = data.approve(&ap, &b_uuid).await?;
        let (vault, log) = data.canonical().await?;
        ensure!(winner.obj_id() == a_request.obj_id().next()
            && vault.0.value.members().len() == 2 && vault.0.value.is_member(data.a.device_id())
            && vault.0.value.is_member(data.b.device_id()) && !vault.0.value.is_member(data.c.device_id())
            && data.server_repo.find_one(queued.event_id.clone()).await?.is_none()
            && !log.0.value.liveboard_removal_results.iter().any(|r| r.request_id == a_uuid),
            "DEPENDENCY actual eligible winner/membership or losing slot differs");
        let canonical_hash = data.hash().await?;
        let mut active_inputs = vec![];
        let shared = PersistentSharedSecret::from(bp.p.clone());
        for pass in &data.passes {
            ensure!(vault.0.value.secrets.contains(pass) && !vault.0.value.deleted_secrets.contains(pass),
                "actual winning Vault no longer contains selected active Pass");
            let packet = shared.get_ss_distribution_event_by_id(SsDistributionId {
                pass_id: pass.clone(), receiver: data.b.device_id().clone(),
            }).await?;
            ensure!(matches!(&packet, SsWorkflowObject::Distribution(x) if x.value.claim_id.pass_id == *pass),
                "actual active B-recipient input identity differs");
            active_inputs.push(packet.to_generic());
        }
        ensure!(active_inputs.len() == 2 && active_inputs[0].obj_id() != active_inputs[1].obj_id(),
            "actual two active B-recipient input packets missing");
        *bp.wire.active_inputs.lock().unwrap() = active_inputs.clone();
        phase = "semantic_retirement";
        eprintln!("CLIENT_EXCLUSIONS changed_context actual_A_winner_Applied=true retained_actual_A_prefix=true C_removed=true A_B_members=true losing_N_empty=true semantic_gate=true");
        bp.select(&a_uuid, None); bp.trace_reset();
        let first = bp.sync().await;
        // Inspect real server and ALL attempts even if this first semantic assertion fails.
        let no_material = baseline.no_material(&bp);
        let canonical_same = data.hash().await? == canonical_hash;
        let business_attempts = bp.wire.actions.lock().unwrap().len();
        let noop_attempts = bp.wire.noops.lock().unwrap().len();
        let mut active_preserved = true;
        for e in &active_inputs { active_preserved &= bp.physical.find_one(e.obj_id()).await?.as_ref() == Some(e); }
        let cut_reached = !bp.wire.cuts.lock().unwrap().is_empty();
        eprintln!("CLIENT_EXCLUSIONS changed_context observed_business_attempts={business_attempts} observed_noop_attempts={noop_attempts} canonical_unchanged={canonical_same} actual_active_B_inputs2_preserved={active_preserved} FIRST_SS_active_snapshot_reached={cut_reached}");
        ensure!(active_preserved, "bounded retirement deleted/changed active actual B input packet");
        fixture::settled(&data, &bp, &baseline, &first).await?;
        ensure!(*bp.wire.active_at_cut.lock().unwrap() == [true], "actual FIRST SS boundary lost active B input bytes/identities");
        no_material?; ensure!(canonical_same, "losing retirement changed actual winner canonical result");
        let settled_hash = digest(&bp.witness(&a_uuid).await?)?;
        bp.trace_reset();
        let retry = bp.sync().await;
        ensure!(bp.wire.stopped_ss(&retry) && data.hash().await? == canonical_hash
            && digest(&bp.witness(&a_uuid).await?)? == settled_hash
            && bp.wire.noops.lock().unwrap().is_empty() && bp.wire.actions.lock().unwrap().is_empty(),
            "settled losing retry rewrote proof or emitted another command");
        ensure!(*bp.wire.active_at_cut.lock().unwrap() == [true], "settled retry lost active B input before FIRST SS");
        baseline.binding(&bp, true).await
    }.await;
    result_line("another_request_winner", phase, &result);
    result
}
