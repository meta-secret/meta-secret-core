use super::*;

async fn blocked_getstate(
    e: &fixture::Env,
    writer_pending: bool,
) -> (
    Pin<Box<impl Future<Output = Result<ApplicationState>> + '_>>,
    Poll<Result<ApplicationState>>,
    bool,
    usize,
    usize,
) {
    let _actual_writer_pending = writer_pending;
    let repo = e.repo.calls.lock().unwrap().len();
    let wire = e.wire.calls.lock().unwrap().len();
    let mut state = Box::pin(e.service.get_app_state());
    let poll = once(state.as_mut()).await;
    let held = e.p.client_operation_guard.try_lock().is_none();
    let calls = e.repo.calls.lock().unwrap().len() - repo;
    let wires = e.wire.calls.lock().unwrap().len() - wire;
    (state, poll, held, calls, wires)
}
async fn finish_state<F: Future<Output = Result<ApplicationState>>>(
    f: Pin<Box<F>>,
    p: Poll<Result<ApplicationState>>,
) -> Result<ApplicationState> {
    match p {
        Poll::Ready(r) => r,
        Poll::Pending => f.await,
    }
}
async fn no_late(e: &fixture::Env, data: &prior::Data, hash: &str) -> Result<usize> {
    let done = proof::cleanup(e).await?;
    ensure!(
        proof::late_writes(e, done) == 0,
        "SEMANTIC old captured writer resurrected local data AFTER actual terminal cleanup"
    );
    proof::unchanged_server(data, hash).await?;
    Ok(done)
}

#[tokio::test]
async fn reset_preservation_real_accept_writer_finishes_before_terminal_cleanup() -> Result<()> {
    let mut phase = "setup_actual_Recovery";
    let result:Result<()>=async {
        let(data,ap,bp)=prior::initial().await?;
        let cp=prior::Peer::from_server(&data,data.c.clone()).await?;
        let e=fixture::Env::wrap(bp).await?;
        let claim=fixture::recovery(&data,&ap,&e,&data.passes[0]).await?;
        let rid=claim.recovery_db_ids().into_iter().find(|r|r.distribution_id.receiver==data.b.device_id().clone()).context("real B-addressed recovery")?;
        let id=ArtifactId::from(SsWorkflowDescriptor::Recovery(rid));
        ensure!(e.peer.physical.find_one(id.clone()).await?.is_none(),"actual B response already installed");
        e.reset();*e.repo.save_point.lock().unwrap()=Some(observer::SavePoint::Recovery(id.clone()));e.repo.save_latch.arm();
        let mut writer=Box::pin(e.service.accept_recover(claim.id.clone()));let writer_poll=once(writer.as_mut()).await;
        let hit=e.repo.save_latch.entered.load(Ordering::SeqCst);
        if !writer_poll.is_pending() || !hit {e.repo.save_latch.release();if writer_poll.is_pending(){let _=writer.await;}anyhow::bail!("DEPENDENCY actual service Recovery save livepoint missing")}
        let setup:Result<String>=async {
            ensure!(e.peer.physical.find_one(id.clone()).await?.is_none(),"barrier forwarded actual response too soon");
            fixture::remove_b_with(&data,&ap,&cp).await?;data.hash().await
        }.await;
        let canonical=match setup {Ok(h)=>h,Err(error)=>{e.repo.save_latch.release();let _=writer.await;return Err(error)}};
        phase="semantic_old_accept_owner";
        let(state,poll,held,calls,wires)=blocked_getstate(&e,true).await;
        let pending=poll.is_pending();let no_cleanup=e.repo.cleans.lock().unwrap().is_empty();
        e.repo.save_latch.release();let writer_result=writer.await;let state_result=finish_state(state,poll).await;
        let physical_absent=e.peer.physical.find_one(id.clone()).await?.is_none();
        let(clean_done,late,before_clean)=proof::writer_order(&e);
        eprintln!("RESET_PRESERVATION Accept actual_cleanup_done={} ALL_actual_forwarded_late_writes={late} real_successful_response_write_before_cleanup={before_clean}",clean_done>0);
        eprintln!("RESET_PRESERVATION Accept actual_response_hook={hit} SAME_client_held={held} GetState_pending={pending} ALL_attributed_repo_calls={calls} ALL_attributed_wire_calls={wires} no_cleanup_before_release={no_cleanup} actual_writer_ok={} actual_GetState_Local={} actual_response_absent_after={physical_absent} SAME_futures_drained=true",writer_result.is_ok(),proof::local(&state_result));
        ensure!(held && pending && calls==0 && wires==0 && no_cleanup,
            "SEMANTIC direct service Accept writer does not exclude automatic terminal cleanup");
        ensure!(writer_result.is_ok() && proof::local(&state_result) && physical_absent && before_clean && late==0,"real response/Reset drain did not preserve write/cleanup order");
        no_late(&e,&data,&canonical).await?;Ok(())
    }.await;
    outcome("actual_Accept_writer", phase, &result);
    result
}

#[tokio::test]
async fn reset_preservation_real_shared_show_writer_and_stale_cached_control() -> Result<()> {
    let mut phase = "setup_real_accepted_threshold_Recovery";
    let result:Result<()>=async {
        let(data,ap,bp)=prior::initial().await?;
        let cp=prior::Peer::from_server(&data,data.c.clone()).await?;
        let e=fixture::Env::wrap(bp).await?;
        // B is the actual requester; A is an addressed eligible approver.
        let receipt=e.service.recover_checked(data.passes[0].clone()).await?;ap.sync().await?;
        let log=PersistentSharedSecret::from(ap.p.clone()).get_ss_log_obj(data.b.vault_name.clone()).await?;
        let claim=log.claims.values().find(|c|receipt.claim_id.as_deref()==Some(c.id.wasm_id_str().as_str())).context("actual B Recovery admitted")?.clone();
        ensure!(claim.sender==*data.b.device_id() && claim.receivers.contains(data.a.device_id()),"real Recovery role binding");
        ap.service().accept_recover(claim.id.clone()).await?;ap.sync().await?;e.gateway.sync(data.b.user()).await?;
        let log=PersistentSharedSecret::from(Arc::new(PersistentObject::new(e.peer.physical.clone()))).get_ss_log_obj(data.b.vault_name.clone()).await?;
        ensure!(log.claims.get(&claim.id).is_some_and(|c|c.compute_client_status(data.b.device_id())==Some(RecoveryClientStatus::Accepted)),"DEPENDENCY real accepted B claim not imported");
        e.reset();*e.repo.save_point.lock().unwrap()=Some(observer::SavePoint::Completed(claim.id.clone(),data.b.device_id().clone()));e.repo.save_latch.arm();
        let cached=e.peer.actor.clone();
        let mut writer=Box::pin(recover_plain_text_by_claim(e.gateway.as_ref(),cached.clone(),claim.id.clone()));
        let writer_poll=once(writer.as_mut()).await;let hit=e.repo.save_latch.entered.load(Ordering::SeqCst);
        if !writer_poll.is_pending() || !hit {e.repo.save_latch.release();if writer_poll.is_pending(){let _=writer.await;}anyhow::bail!("DEPENDENCY actual threshold-decrypt/local Show completion save livepoint missing")}
        // The production handler reached mark_claim_delivered only after real threshold decryption.
        let setup:Result<String>=async {fixture::remove_b_with(&data,&ap,&cp).await?;data.hash().await}.await;
        let canonical=match setup {Ok(h)=>h,Err(error)=>{e.repo.save_latch.release();let _=writer.await;return Err(error)}};
        phase="semantic_old_Show_owner";
        let(state,poll,held,calls,wires)=blocked_getstate(&e,true).await;
        let pending=poll.is_pending();let no_cleanup=e.repo.cleans.lock().unwrap().is_empty();
        e.repo.save_latch.release();let plain=writer.await;let state_result=finish_state(state,poll).await;
        let correct_plain=plain.as_ref().is_ok_and(|p|p.text=="test-only-initial-value");
        let(clean_done,late,before_clean)=proof::writer_order(&e);
        eprintln!("RESET_PRESERVATION Show actual_cleanup_done={} ALL_actual_forwarded_late_writes={late} real_successful_completion_write_before_cleanup={before_clean}",clean_done>0);
        eprintln!("RESET_PRESERVATION Show actual_threshold_decrypt_and_mark_hook={hit} SAME_client_held={held} GetState_pending={pending} ALL_attributed_repo_calls={calls} ALL_attributed_wire_calls={wires} no_cleanup_before_release={no_cleanup} legitimate_plaintext_correct={correct_plain} actual_GetState_Local={} SAME_futures_drained=true",proof::local(&state_result));
        ensure!(held && pending && calls==0 && wires==0 && no_cleanup,"SEMANTIC shared local Show writer does not exclude terminal cleanup");
        ensure!(correct_plain && proof::local(&state_result) && before_clean && late==0,"actual legitimate Show/Reset drain/order failed");
        no_late(&e,&data,&canonical).await?;
        phase="stale_cached_control";let before=e.peer.physical.get_db().await;let cursor=proof::count_writes(&e);
        let stale=recover_plain_text_by_claim(e.gateway.as_ref(),cached,claim.id).await;
        ensure!(stale.is_err() && proof::count_writes(&e)==cursor && e.peer.physical.get_db().await==before,
            "old cached helper arguments recreated state after actual Reset");
        eprintln!("RESET_PRESERVATION stale_cached_after_actual_cleanup_error=true ALL_new_writes0=true no_records_recreated=true");Ok(())
    }.await;
    outcome("actual_shared_Show_writer", phase, &result);
    result
}
