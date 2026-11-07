use super::*;
use meta_secret_core::node::db::events::vault::vault_log_event::RemoveMemberDecision;

#[tokio::test]
async fn p1_two_secret_real_own_accepted_publishes_exact_frozen_generation_after_local_applied()
-> Result<()> {
    let case = fixture::Case::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let before = case.h.p.repo.snapshot().await?;
    let (_, frozen) = case.prepare(true).await?;
    super::super::stage::private_crypto(&case.h.state, &case.h.witness().await?)?;
    ensure!(
        ordinary(&case.h.p.repo.snapshot().await?) == ordinary(&before),
        "C2 preparation changed live K2"
    );
    let sync = case.gateway().sync(case.h.state.b.user()).await;
    let measured = proof::measure(&case).await?;
    measured.print("P1 actual ownAccepted TWO Secrets", &sync);
    // Gather all reachable actual phases BEFORE the first specification failure.
    proof::common(&measured)?;
    sync?;
    proof::published(&case, &frozen).await
}

#[tokio::test]
async fn p2_actual_decline_has_complete_empty_stage_real_declined_three_members_and_no_k1()
-> Result<()> {
    let case = fixture::Case::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let before = case.h.p.repo.snapshot().await?;
    let prepared = case.prepare(false).await;
    eprintln!("C2 P2 actual Decline prequeue_ok={}", prepared.is_ok());
    let (_, frozen) = prepared?;
    ensure!(
        frozen.update.decision == RemoveMemberDecision::Declined && frozen.passes.is_empty(),
        "C2 Decline must have COMPLETE update and zero K1 Passes"
    );
    ensure!(
        ordinary(&case.h.p.repo.snapshot().await?) == ordinary(&before),
        "C2 Decline preparation changed ordinary material"
    );
    let sync = case.gateway().sync(case.h.state.b.user()).await;
    let measured = proof::measure(&case).await?;
    measured.print("P2 actual Decline", &sync);
    let writes = case.observations.live.lock().unwrap().clone();
    let live_unchanged = writes
        .iter()
        .all(|w| before.get(&w.packet.obj_id()) == Some(&w.packet.clone().to_generic()));
    let own_claims_unchanged = case.h.p.repo.snapshot().await?.values().all(|x| {
        !matches!(x, GenericKvLogEvent::SsDeviceLog(c) if c.0.value.sender == *case.h.state.b.device_id())
    });
    eprintln!(
        "C2 P2 live_original_only={live_unchanged} own_new_claim_absent={own_claims_unchanged}"
    );
    proof::common(&measured)?;
    sync?;
    ensure!(
        live_unchanged && own_claims_unchanged,
        "C2 Decline created/replaced K1 or new normal generation"
    );
    for pass in &case.h.state.passes {
        let original = before.values().find(|event| matches!(event, GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(p))
            if p.value.claim_id.pass_id == *pass && p.value.secret_message.cipher_text().channel.receiver().to_device_id() == *case.h.state.b.device_id()))
            .context("actual original B packet")?;
        ensure!(
            case.h
                .p
                .repo
                .inner
                .inner
                .find_one(original.obj_id())
                .await?
                == Some(original.clone()),
            "C2 Decline replaced actual original K2 packet"
        );
    }
    Ok(())
}

#[tokio::test]
async fn p3_initial_empty_three_member_vault_accepts_actual_signed_removal_without_fictitious_passes()
-> Result<()> {
    let case = fixture::Case::empty(Arc::new(InMemKvLogEventRepo::default())).await?;
    ensure!(case.h.state.passes.is_empty(), "C2 empty INITIAL fixture");
    let prepared = case.prepare(true).await;
    eprintln!(
        "C2 P3 legitimate INITIAL empty prequeue_ok={}",
        prepared.is_ok()
    );
    let (_, frozen) = prepared?;
    ensure!(
        frozen.passes.is_empty() && frozen.update.request.share_transfers.is_empty(),
        "C2 empty initial Vault generated fictitious inputs"
    );
    let sync = case.gateway().sync(case.h.state.b.user()).await;
    let measured = proof::measure(&case).await?;
    measured.print("P3 initial empty ownAccepted", &sync);
    let empty_outputs = proof::empty_outputs(&case).await?;
    eprintln!(
        "C2 P3 all-current-Vault output oracle clean={empty_outputs} claim_record_attempts={}",
        case.observations.claim_writes.lock().unwrap().len()
    );
    proof::common(&measured)?;
    sync?;
    ensure!(
        measured.live == 0 && case.observations.claims.lock().unwrap().is_empty() && empty_outputs,
        "C2 empty accepted removal fabricated publication"
    );
    Ok(())
}
