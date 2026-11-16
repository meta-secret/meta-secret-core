use super::*;

#[tokio::test]
async fn reset_preservation_same_public_sync_and_fifo_credential_owner_interval() -> Result<()> {
    let mut phase = "setup_actual_nooutbox_removal";
    let result:Result<()>=async {
        let(data,e)=fixture::removed_plain().await?;let canonical=data.hash().await?;
        e.wire.tail_latch.arm();e.repo.clean_latch.arm();
        let mut first_sync=Box::pin(e.gateway.sync(e.peer.actor.user()));
        let first_poll=once(first_sync.as_mut()).await;
        if !first_poll.is_pending() || !e.wire.tail_latch.entered.load(Ordering::SeqCst) {
            e.wire.tail_latch.release();
            if first_poll.is_pending(){let _=first_sync.await;}
            anyhow::bail!("DEPENDENCY actual first sync livepoint missing")
        }
        let wire_cursor=e.wire.calls.lock().unwrap().len();
        let mut state=Box::pin(e.service.get_app_state());let state_first=once(state.as_mut()).await;
        let entry_pending=state_first.is_pending();let entry_wire=e.wire.calls.lock().unwrap().len()-wire_cursor;
        let no_entry_cleanup=e.repo.cleans.lock().unwrap().is_empty() && !e.repo.clean_latch.entered.load(Ordering::SeqCst);
        e.wire.tail_latch.release();let first_result=first_sync.await;
        let mut early=match state_first {Poll::Ready(r)=>Some(r),Poll::Pending=>None};
        if early.is_none() && !e.repo.clean_latch.entered.load(Ordering::SeqCst)
            && let Poll::Ready(r)=once(state.as_mut()).await
        {early=Some(r);}
        let reached=e.repo.clean_latch.entered.load(Ordering::SeqCst);
        if !reached || early.is_some() {
            e.repo.clean_latch.release();
            if early.is_none(){let _=state.await;}
            anyhow::bail!("DEPENDENCY actual GetState cleanup livepoint unreachable after real sync release")
        }
        phase="semantic_cleanup_owner";
        let physical_before=e.peer.physical.get_db().await;
        let owner_client=e.p.client_operation_guard.try_lock().is_none();let owner_sync=e.p.device_sync_guard.try_lock().is_none();
        let repo_cursor=e.repo.calls.lock().unwrap().len();let wire_cursor=e.wire.calls.lock().unwrap().len();
        let mut second_sync=Box::pin(e.gateway.sync(e.peer.actor.user()));let second_poll=once(second_sync.as_mut()).await;
        let new_name=meta_secret_core::node::common::model::vault::vault::VaultName::from("reset-next-real-session");
        let mut generate=Box::pin(e.service.handle_client_request(ApplicationState::Local(e.peer.actor.device()),GenericAppStateRequest::GenerateUserCreds(new_name.clone())));
        let generate_poll=once(generate.as_mut()).await;
        let phase_b_pending=second_poll.is_pending() && generate_poll.is_pending();
        let phase_b_calls=e.repo.calls.lock().unwrap().len()-repo_cursor;
        let phase_b_wire=e.wire.calls.lock().unwrap().len()-wire_cursor;
        let original_unchanged=e.peer.physical.get_db().await==physical_before;
        // Release and drain SAME production futures even when the ownership oracle is RED.
        e.repo.clean_latch.release();let state_result=state.await;
        let second_result=match second_poll {Poll::Ready(r)=>r,Poll::Pending=>second_sync.await};
        let generated=match generate_poll {Poll::Ready(r)=>r,Poll::Pending=>generate.await};
        let current=e.repo.raw_creds().await?;
        let fresh_actual_session=current.as_ref().is_some_and(|c|c.vault_name==new_name && *c!=e.peer.actor);
        eprintln!("RESET_PRESERVATION owner entry_pending={entry_pending} entry_wire={entry_wire} no_entry_cleanup={no_entry_cleanup} cleanup_livepoint={reached} SAME_client_held={owner_client} SAME_sync_held={owner_sync} phaseB_pending={phase_b_pending} ALL_phaseB_repo_calls={phase_b_calls} ALL_phaseB_wire_calls={phase_b_wire} olddata_unchanged={original_unchanged} first_sync_ok={} second_sync_ok={} state_Local={} actual_generation_ok={} real_new_session_persisted={fresh_actual_session} SAME_futures_drained=true",first_result.is_ok(),second_result.is_ok(),proof::local(&state_result),generated.is_ok());
        ensure!(entry_pending && entry_wire==0 && no_entry_cleanup && reached && owner_client && owner_sync
            && phase_b_pending && phase_b_calls==0 && phase_b_wire==0 && original_unchanged,
            "SEMANTIC automatic cleanup lacks SAME client/sync exclusion versus real sync/credential writer");
        ensure!(first_result.is_ok() && second_result.is_ok() && proof::local(&state_result)
            && generated.is_ok() && fresh_actual_session,"owner futures failed to complete real existing behavior after release");
        {
            let cleans=e.repo.cleans.lock().unwrap();
            ensure!(cleans.len()==1 && cleans[0].after_empty,"actual old cleanup not completed exactly once");
        }
        proof::unchanged_server(&data,&canonical).await?;proof::audit(&e)
    }.await;
    outcome("same_owner_interval", phase, &result);
    result
}
