use super::*;

#[tokio::test]
async fn owned_lifetime_packetbearing_previous_noop_current_applied_member2_then_real_rejoin3()
-> Result<()> {
    let mut phase = "actual_packetbearing_N_Noop_explicit_Accepted_Nplus1";
    let result: Result<()> = async {
        let h = harness::Harness::fresh(true).await?;
        phase = "actual_Member2_GetState_twice";
        let compact = proof::member2_twice(&h).await?;
        let mut terminal = compact.clone();
        terminal.0.value.payload = None;
        terminal.0.value.phase = GuardedPreparationPhase::BlockedRetiredCommand;
        let pass = h.case.h.state.passes[1].clone();
        let initial = h.local.inner.find_one(packet_id(&pass, &h.case.h.state.b)).await?
            .context("actual Applied member2 B-owned copy")?.shared_secret()?;
        phase = "actual_signed_rejoin_eligible_approval";
        let (a, c) = peers::rejoin_c(&h).await?;
        let start = h.repo.tick();
        phase = "FIRST_whole_public_sync_after_real_Join3";
        let sync = h.sync().await;
        let unavailable = sync.as_ref().err().is_some_and(|e| e.chain().any(|x| x.to_string() == "GuardedPreparationUnavailable"));
        eprintln!("OWNED_LIFETIME JOIN FIRST_whole_sync_ok={} actual_guard_unavailable={unavailable}", sync.is_ok());
        // Do not consume failure and hide it behind another retry or allowed error.
        sync.context("SEMANTIC actual old Applied2 context blocks legitimate signed Join3")?;
        {
            let writes = h.repo.writes.lock().unwrap();
            let first_output = writes.iter().filter(|w| w.forward > start && matches!(w.event,
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_)) | GenericKvLogEvent::SsDeviceLog(_)))
                .map(|w| w.forward).min().context("actual Join resharing outputs UNREACHED")?;
            {
                let reads = h.repo.reads.lock().unwrap();
                let terminal_read = reads.iter().find(|r| r.order > start && r.order < first_output
                    && r.event.as_ref() == Some(&terminal.clone().to_generic()))
                    .context("exact terminal compact successful production readback before fresh Join outputs")?;
                // Production Join source audit/ordinary WholeCopy loader reads this real input.
                // This is the bounded read/output call-graph proxy, not an RNG/phase hook.
                ensure!(reads.iter().any(|r| r.order > terminal_read.order && r.order < first_output
                    && r.id == initial.obj_id() && r.event.as_ref() == Some(&initial.clone().to_generic())),
                    "actual source-bound ordinary WholeCopy Join input not read AFTER exact terminal readback");
            }
            ensure!(writes.iter().filter(|w| w.forward > start).all(|w| !matches!(&w.event,
                GenericKvLogEvent::LocalGuardedPreparation(p) if p.0.value.payload.is_some())), "retired FULL resurrected during Join");
        }
        phase = "real_peer_delivery_crypto_and_repeat";
        a.pull_vault(&h).await?;
        c.pull_vault(&h).await?;
        a.pull_ss(&h).await?;
        c.pull_ss(&h).await?;
        let actual_b = h.local.inner.find_one(packet_id(&pass, &h.case.h.state.b)).await?
            .context("actual current B K2")?.shared_secret()?;
        ensure!(actual_b != initial, "real Join did not replace member2 K1");
        crypto::all_pairs(&h, &a, &c, &pass, &actual_b).await?;
        let baseline = proof::claim_baseline(&h.local.inner.get_db().await)?;
        let rc = h.repo.calls.lock().unwrap().len();
        let wc = h.wire.calls.lock().unwrap().len();
        h.sync().await?;
        let state = h.service().get_app_state().await?;
        ensure!(matches!(state, ApplicationState::Vault(VaultFullInfo::Member(m)) if m.member.vault.members().len() == 3),
            "actual postJoin GetState not Member3");
        proof::no_new_material(&h, rc, wc, false, &baseline)?;
        ensure!(h.case.h.witness().await? == terminal, "Join/delivery/repeat mutated full terminal current+previous compact");
        ensure!(h.local.inner.find_one(packet_id(&pass, &h.case.h.state.b)).await? == Some(actual_b.to_generic()),
            "repeat changed current K2 generation");
        for peer in [&a, &c] {
            let id = packet_id(&pass, &peer.creds);
            ensure!(h.local.inner.find_one(id.clone()).await?.is_none()
                && h.case.h.state.server_repo.find_one(id).await?.is_none(), "delivered peer ciphertext resurrected");
        }
        ensure!(checks::ordinary_clean(&h.local.inner.get_db().await, &h.case.h.state.passes[0]), "p0 restored after Join");
        eprintln!("OWNED_LIFETIME JOIN actual_K2_pairs_singletons=true actual_peer_delivery=true repeat_GetState_ALL_peer_attempts0=true");
        Ok(())
    }.await;
    eprintln!("OWNED_LIFETIME JOIN phase={phase} ok={}", result.is_ok());
    result
}
