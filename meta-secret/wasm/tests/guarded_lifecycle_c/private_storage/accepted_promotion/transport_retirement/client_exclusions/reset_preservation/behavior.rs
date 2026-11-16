use super::*;

#[tokio::test]
async fn reset_preservation_actual_removed_noop_error_returns_local_and_cleans() -> Result<()> {
    let mut phase = "setup";
    let result:Result<()>=async {
        let(data,e,auth)=fixture::removed_noop().await?;let canonical=data.hash().await?;
        phase="semantic_actual_GetState";
        let actual=e.service.get_app_state().await;
        let unauthorized=e.wire.unauthorized.lock().unwrap().len();
        let physical_status=e.peer.physical.get_db().await.values().any(|v|matches!(v,GenericKvLogEvent::VaultStatus(s)
            if matches!(s.clone().status(),VaultStatus::Outsider(o) if o.user_data==data.b.user() && o.status==UserDataOutsiderStatus::Declined)));
        eprintln!("RESET_PRESERVATION removed GetState_called=true actual_Unauthorized={unauthorized} persisted_Declined={physical_status} actual_Local={} actual_cleanup_count={}",proof::local(&actual),e.repo.cleans.lock().unwrap().len());
        ensure!(unauthorized==1,"DEPENDENCY actual GetState did not reach real server Unauthorized");
        proof::unchanged_server(&data,&canonical).await?;
        ensure!(data.server_repo.find_one(auth.event_id.clone()).await?.is_none(),"removed GetState falsely settled transport N");
        ensure!(proof::local(&actual),"SEMANTIC removed GetState propagated irrelevant sync error before existing Reset");
        let done=proof::cleanup(&e).await?;
        ensure!(proof::late_writes(&e,done)==0,"terminal Reset followed by local write");
        let direct=e.service.get_app_state().await?;
        let owned=e.service.handle_client_request(ApplicationState::Local(e.peer.actor.device()),GenericAppStateRequest::GetState).await?;
        let startup=e.service.build_service_state().await?;
        ensure!(matches!(direct,ApplicationState::Local(_)) && matches!(owned,ApplicationState::Local(_))
            && matches!(startup.app_state,ApplicationState::Local(_)) && e.repo.cleans.lock().unwrap().len()==1,
            "postReset direct/already-owned/startup state builder regressed or repeated cleanup");
        proof::unchanged_server(&data,&canonical).await
    }.await;
    outcome("removed_noop_Reset", phase, &result);
    result
}

#[tokio::test]
async fn reset_preservation_active_member_keeps_original_sync_error_and_local_data() -> Result<()> {
    let mut phase = "setup";
    let result: Result<()> = async {
        let (data, _ap, bp) = prior::initial().await?;
        let e = fixture::Env::wrap(bp).await?;
        let before = e.peer.physical.get_db().await;
        let canonical = data.hash().await?;
        ensure!(
            before.values().any(|v| matches!(
                v,
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_))
            )),
            "actual member material missing"
        );
        e.wire.tail_error.store(true, Ordering::SeqCst);
        phase = "actual_original_error";
        let actual = e.service.get_app_state().await;
        proof::original_error(&e, &actual, &before).await?;
        proof::unchanged_server(&data, &canonical).await?;
        proof::audit(&e)
    }
    .await;
    outcome("member_original_error", phase, &result);
    result
}

#[tokio::test]
async fn reset_preservation_real_declined_join_candidate_keeps_credentials() -> Result<()> {
    let mut phase = "setup_real_Join";
    let result:Result<()>=async {
        let(data,_ap,_bp)=prior::initial().await?;let e=fixture::candidate(&data).await?;
        ensure!(!e.peer.physical.get_db().await.values().any(|v|matches!(v,GenericKvLogEvent::Vault(v)
            if v.0.value.is_member(e.peer.actor.device_id()))),"candidate has fabricated prior Member evidence");
        phase="candidate_GetState";
        let actual=e.service.get_app_state().await?;
        ensure!(matches!(actual,ApplicationState::Vault(VaultFullInfo::Outsider(o))
            if o.user_data==e.peer.actor.user() && o.status==UserDataOutsiderStatus::Declined)
            && e.repo.cleans.lock().unwrap().is_empty() && e.repo.raw_creds().await?.as_ref()==Some(&e.peer.actor),
            "declined Join candidate reset or lost credentials");
        let before=e.peer.physical.get_db().await;let canonical=data.hash().await?;
        e.reset();e.wire.tail_error.store(true,Ordering::SeqCst);
        let error=e.service.get_app_state().await;
        proof::original_error(&e,&error,&before).await?;proof::unchanged_server(&data,&canonical).await?;
        eprintln!("RESET_PRESERVATION real_Join_declined=true priorMember=false actual_Outsider_retained=true credentials_retained=true");
        proof::audit(&e)
    }.await;
    outcome("declined_Join_retention", phase, &result);
    result
}
