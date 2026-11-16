use super::*;
#[tokio::test]
async fn mobile_presync_reset_actual_removed_noop_returns_local_without_second_pass() -> Result<()>
{
    let mut phase = "setup_before_prepare";
    let result:Result<()>=async{
 let(case,data,e)=fixture::before_prepare().await?;let mut run=Box::pin(e.service.run());runtime::startup(&e,run.as_mut(),true).await?;
 let setup=fixture::stage_removed(&case,&data,&e).await;let auth=match setup{Ok(a)=>a,Err(err)=>{drop(run);runtime::closed(&e)?;return Err(err)}};
 let canonical=data.hash().await?;phase="semantic_actual_manager";let actual=runtime::manager(&e,run.as_mut()).await;
 let idle=runtime::idle_poll(&e,run.as_mut()).await;drop(run);runtime::closed(&e)?;
 let unauthorized=e.wire.unauthorized.lock().unwrap().clone();let declined=e.peer.physical.get_db().await.values().any(|v|matches!(v,GenericKvLogEvent::VaultStatus(s)if matches!(s.clone().status(),VaultStatus::Outsider(o)if o.user_data==data.b.user()&&o.status==UserDataOutsiderStatus::Declined)));
 eprintln!("MOBILE_PRESYNC_RESET removed real_Unauthorized={} persisted_Declined={declined} manager_Local={} cleanup={} actual_Tail={} actual_SS={} idle_noeffects={} run_Closed=true",unauthorized.len(),proof::local(&actual),e.repo.cleans.lock().unwrap().len(),e.wire.tails(),e.wire.ss(),idle.is_ok());
 let imported=proof::imported_declined(&e);eprintln!("MOBILE_PRESYNC_RESET exact_imported_Declined_outer_write_read={imported}");
 ensure!(unauthorized.len()==1&&imported,"DEPENDENCY actual manager first Noop/error/imported status missing");proof::original_transport(&e,&auth)?;ensure!(data.hash().await?==canonical&&data.server_repo.find_one(auth.event_id.clone()).await?.is_none(),"manager changed canonical/Noop slot");
 proof::no_material(&e,unauthorized[0])?;idle?;ensure!(proof::local(&actual),"SEMANTIC real mobile manager returns pre-sync error before proven service Reset");
 ensure!(e.wire.tails()==1&&e.wire.ss()==0,"terminal manager performed a normal second sync");proof::cleanup(&e,unauthorized[0]).await?;proof::audit(&e)
 }.await;
    result_line_mobile("removed_NoOp", phase, &result);
    result
}
async fn ordinary(candidate: bool) -> Result<()> {
    let (data, _a, b) = prior::initial().await?;
    let e = if candidate {
        fixture::candidate(&data).await?
    } else {
        fixture::Env::wrap(b).await?
    };
    let mut run = Box::pin(e.service.run());
    runtime::startup(&e, run.as_mut(), !candidate).await?;
    let before = e.peer.physical.get_db().await;
    let canonical = data.hash().await?;
    e.reset();
    e.wire.tail_error.store(true, Ordering::SeqCst);
    let actual = runtime::manager(&e, run.as_mut()).await;
    let idle = runtime::idle_poll(&e, run.as_mut()).await;
    drop(run);
    runtime::closed(&e)?;
    ensure!(
        actual.as_ref().err().is_some_and(|e| e.is::<TailError>())
            && e.wire.same_error(&actual)
            && e.repo.cleans.lock().unwrap().is_empty()
            && e.peer.physical.get_db().await == before
            && data.hash().await? == canonical
            && e.wire.calls.lock().unwrap().len() == 1,
        "manager replaced original ordinary error/changed DB/added effects"
    );
    idle?;
    proof::audit(&e)?;
    eprintln!(
        "MOBILE_PRESYNC_RESET ordinary candidate={candidate} ORIGINAL_type_chain=true DBexact=true cleanup0=true wire1=true idle_noeffects=true Closed=true"
    );
    Ok(())
}
async fn proof_fault() -> Result<()> {
    let (case, data, e) = fixture::before_prepare().await?;
    let mut run = Box::pin(e.service.run());
    runtime::startup(&e, run.as_mut(), true).await?;
    let staged = fixture::stage_removed(&case, &data, &e).await;
    let auth = match staged {
        Ok(auth) => auth,
        Err(err) => {
            drop(run);
            runtime::closed(&e)?;
            return Err(err);
        }
    };
    let canonical = data.hash().await?;
    e.repo.fault.store(true, Ordering::SeqCst);
    let actual = runtime::manager(&e, run.as_mut()).await;
    let idle = runtime::idle_poll(&e, run.as_mut()).await;
    drop(run);
    runtime::closed(&e)?;
    let hit = e.repo.fault_hit.load(Ordering::SeqCst);
    eprintln!(
        "MOBILE_PRESYNC_RESET proof_fault actual_Unauthorized={} exact_terminal_client_sync_fault_hit={hit} original_error={} cleanup={} run_Closed=true",
        e.wire.unauthorized.lock().unwrap().len(),
        e.wire.same_error(&actual),
        e.repo.cleans.lock().unwrap().len()
    );
    ensure!(
        e.wire.unauthorized.lock().unwrap().len() == 1
            && e.wire.same_error(&actual)
            && e.repo.cleans.lock().unwrap().is_empty()
            && data.hash().await? == canonical,
        "proof fault changed original first error/cleanup/server"
    );
    idle?;
    let boundary = e.wire.unauthorized.lock().unwrap()[0];
    proof::no_material(&e, boundary)?;
    proof::original_transport(&e, &auth)?;
    ensure!(
        e.wire.tails() == 1 && e.wire.ss() == 0,
        "proof failure ran a normal second network pass"
    );
    ensure!(
        hit && e.repo.fault_owner.load(Ordering::SeqCst),
        "DEPENDENCY terminal proof fault UNREACHED before real mobile handoff exists"
    );
    let fault_snapshot = { e.repo.fault_snapshot.lock().unwrap().clone() };
    ensure!(
        fault_snapshot.as_ref() == Some(&e.peer.physical.get_db().await),
        "proof failure changed physical data"
    );
    proof::audit(&e)
}
#[tokio::test]
async fn mobile_presync_reset_original_errors_and_selected_proof_fault_table() -> Result<()> {
    let a = ordinary(false).await;
    result_line_mobile("Member_original_error", "actual_manager", &a);
    let b = ordinary(true).await;
    result_line_mobile("real_declined_Join_error", "actual_manager", &b);
    let c = proof_fault().await;
    result_line_mobile(
        "terminal_proof_read_error",
        "actual_handoff_or_UNREACHED",
        &c,
    );
    ensure!(
        a.is_ok() && b.is_ok() && c.is_ok(),
        "bounded table has failed/unreached variants; all3 collected"
    );
    Ok(())
}
#[tokio::test]
async fn mobile_presync_reset_healthy_recovery_preserves_two_passes_and_real_fifo() -> Result<()> {
    let mut phase = "setup_actual_Recovery";
    let result:Result<()>=async{
 let(data,ap,bp)=prior::initial().await?;let e=fixture::Env::wrap(bp).await?;let mut run=Box::pin(e.service.run());runtime::startup(&e,run.as_mut(),true).await?;
 let staged:Result<_>=async{let receipt=ap.service().recover_checked(data.passes[0].clone()).await?;e.gateway.sync(data.b.user()).await?;let log=PersistentSharedSecret::from(e.p.clone()).get_ss_log_obj(data.b.vault_name.clone()).await?;let claim=log.claims.values().find(|c|receipt.claim_id.as_deref()==Some(c.id.wasm_id_str().as_str())).context("actual addressed Recovery not imported")?.clone();ensure!(claim.sender==*data.a.device_id()&&claim.receivers.contains(data.b.device_id()),"actual Recovery role binding");e.service.accept_recover(claim.id.clone()).await?;let rid=claim.recovery_db_ids().into_iter().find(|r|r.distribution_id.receiver==*data.b.device_id()).context("real B response ID")?;let id=ArtifactId::from(SsWorkflowDescriptor::Recovery(rid));let workflow=e.peer.physical.find_one(id.clone()).await?.context("actual queued B Recovery response absent")?;ensure!(data.server_repo.find_one(id.clone()).await?.is_none(),"B Recovery response already on server");Ok((claim,id,workflow))}.await;
 let(claim,id,workflow)=match staged{Ok(v)=>v,Err(err)=>{drop(run);runtime::closed(&e)?;return Err(err)}};
 let canonical=data.hash().await?;let physical_before=e.peer.physical.get_db().await;let claims_before=proof_claims(&physical_before)?;e.reset();phase="actual_manager_two_pass_control";
 let actual=runtime::manager(&e,run.as_mut()).await;drop(run);runtime::closed(&e)?;
 let (signed, order, ack, upload_count, tails) = {
     let replies=e.wire.replies.lock().unwrap();let uploads=replies.iter().filter_map(|r|match &r.request{SyncRequest::Write(w)=>match &**w{WriteSyncRequest::Event(action)=>match &action.payload{meta_secret_core::node::api::SignedActionPayload::Event(ev)if ev.obj_id()==id=>Some((r,action,ev)),_=>None},_=>None},_=>None}).collect::<Vec<_>>();
     let ss=replies.iter().filter(|r|matches!(&r.request,SyncRequest::Read(q)if matches!(&**q,ReadSyncRequest::SsRequest(_)))).collect::<Vec<_>>();let tails=replies.iter().filter(|r|matches!(&r.request,SyncRequest::Read(q)if matches!(&**q,ReadSyncRequest::ServerTail(_)))).count();
     let signed=uploads.iter().all(|(_,action,ev)|action.signer==*data.b.device_id()&&*ev==&workflow&&verify_action(action,&data.b.user().device.keys.dsa_pk).is_ok());
     let calls=e.wire.calls.lock().unwrap();
     let ss_calls=calls.iter().filter(|r|matches!(&r.request,SyncRequest::Read(q)if matches!(&**q,ReadSyncRequest::SsRequest(_)))).collect::<Vec<_>>();
     let tail_calls=calls.iter().filter(|r|matches!(&r.request,SyncRequest::Read(q)if matches!(&**q,ReadSyncRequest::ServerTail(_)))).collect::<Vec<_>>();
     let tail_replies=replies.iter().filter(|r|matches!(&r.request,SyncRequest::Read(q)if matches!(&**q,ReadSyncRequest::ServerTail(_)))).collect::<Vec<_>>();
     let order=uploads.len()==1&&ss.len()==2&&ss_calls.len()==2&&tail_calls.len()==2&&tail_replies.len()==2
      &&tail_calls[0].order<tail_replies[0].order&&tail_replies[0].order<uploads[0].0.order
      &&uploads[0].0.order<ss_calls[0].order&&ss_calls[0].order<ss[0].order
      &&ss[0].order<tail_calls[1].order&&tail_calls[1].order<tail_replies[1].order
      &&tail_replies[1].order<ss_calls[1].order&&ss_calls[1].order<ss[1].order;
     let ack=uploads.iter().all(|(r,_,_)|r.response.clone().ensure_success().is_ok());let upload_count=uploads.len();
     (signed, order, ack, upload_count, tails)
 };
 let projected=matches!(&actual,Ok(ApplicationState::Vault(VaultFullInfo::Member(m)))if m.member.member.user_data==data.b.user()&&m.ss_claims.claims.get(&claim.id).is_some_and(|c|c.compute_client_status(data.a.device_id())==Some(RecoveryClientStatus::Accepted)));
 let server=PersistentSharedSecret::from(Arc::new(PersistentObject::new(data.server_repo.clone()))).get_ss_log_obj(data.b.vault_name.clone()).await?;let accepted=server.claims.get(&claim.id).is_some_and(|c|c.compute_client_status(data.a.device_id())==Some(RecoveryClientStatus::Accepted));
 let projection_matches=match (&actual,server.claims.get(&claim.id)){(Ok(ApplicationState::Vault(VaultFullInfo::Member(m))),Some(canonical))=>m.ss_claims.claims.get(&claim.id).is_some_and(|selected|claim_identity(selected).ok()==claim_identity(canonical).ok()&&selected.status==canonical.status&&selected.client_status==canonical.compute_client_status(data.b.device_id())),_=>false};
 eprintln!("MOBILE_PRESYNC_RESET healthy actual_B_client_projection_matches_server={projection_matches}");
 eprintln!("MOBILE_PRESYNC_RESET healthy manager_Member_projection={projected} actual_Tail_successes={tails} actual_SS_successes={} exact_uploads={} actual_B_signature={signed} real_ACK={ack} upload_before_first_SS={order} server_Accepted={accepted} outbound_removed={} SAME_run_reply_drained_Closed=true",e.wire.ss(),upload_count,e.peer.physical.find_one(id.clone()).await?.is_none());
 ensure!(projected&&projection_matches&&tails==2&&e.wire.ss()==2&&signed&&ack&&order&&accepted&&e.peer.physical.find_one(id).await?.is_none(),"healthy actual manager lost real upload/FIFO/projection/two-pass order");ensure!(data.hash().await?==canonical&&proof_claims(&e.peer.physical.get_db().await)?==claims_before,"healthy manager introduced new canonical/generation/claim identity");no_new_attempts(&e,&physical_before,&claims_before)?;proof::audit(&e)
 }.await;
    result_line_mobile("healthy_two_pass_Recovery", phase, &result);
    result
}
fn proof_claims(db: &HashMap<ArtifactId, GenericKvLogEvent>) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    for e in db.values() {
        match e {
            GenericKvLogEvent::SsLog(l) => {
                for c in l.0.value.claims.values() {
                    out.insert(c.id.wasm_id_str(), claim_identity(c)?);
                }
            }
            GenericKvLogEvent::SsDeviceLog(l) => {
                let c = &l.0.value;
                out.insert(c.id.wasm_id_str(), claim_identity(c)?);
            }
            _ => {}
        }
    }
    Ok(out)
}

fn no_new_attempts(
    e: &fixture::Env,
    before: &HashMap<ArtifactId, GenericKvLogEvent>,
    claims: &HashMap<String, String>,
) -> Result<()> {
    let mut distributions = 0;
    let mut claim_writes = 0;
    for c in e.repo.calls.lock().unwrap().iter() {
        let observer::Op::Write(_, event) = &c.op else {
            continue;
        };
        let event = event.as_ref();
        match event {
            GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_)) => {
                distributions += 1;
                ensure!(
                    before.get(&event.obj_id()) == Some(event),
                    "healthy no-topology manager wrote new/recomputed Distribution"
                );
            }
            GenericKvLogEvent::SsLog(l) => {
                for claim in l.0.value.claims.values() {
                    claim_writes += 1;
                    ensure!(
                        claims.get(&claim.id.wasm_id_str()) == Some(&claim_identity(claim)?),
                        "healthy manager attempted new claim/generation before later cleanup"
                    );
                }
            }
            GenericKvLogEvent::SsDeviceLog(l) => {
                claim_writes += 1;
                let claim = &l.0.value;
                ensure!(
                    claims.get(&claim.id.wasm_id_str()) == Some(&claim_identity(claim)?),
                    "healthy manager attempted new sequential claim/generation"
                );
            }
            _ => {}
        }
    }
    eprintln!(
        "MOBILE_PRESYNC_RESET healthy ALL_Distribution_attempts={distributions} ALL_claim_attempts={claim_writes} no_new_claim_or_generation=true"
    );
    Ok(())
}
