use super::*;
#[tokio::test]
async fn c4_incoming_current_material_first_full_sync_imports_k2_and_preserves_retired_compact_through_getstate()
-> Result<()> {
    let mut phase = "real_setup";
    let result:Result<()>=async{
 let(case,local)=super::super::case().await?;let before=local.inner.get_db().await;let current=crypto::capture(&case,&local.inner).await?;let claims=proof::baseline_claims(&before)?;
 let(receipt,payload)=case.prepare(true).await?;let original=case.h.witness().await?;private_crypto(&case.h.state,&original)?;
 let origin=original.0.value.origin.clone().context("actual Accepted N origin")?;let body=local.inner.find_one(receipt.event_id.clone()).await?.context("actual queued B:N")?.device_log()?;
 ensure!(origin.accepted&&origin.event_id==body.obj_id()&&producer_source_commitment(&body)?==origin.body_commitment,"actual Accepted queued N binding");
 let planned=payload.passes.iter().map(|p|p.claim.dist_claim_id.clone()).collect::<Vec<_>>();let pass=payload.passes.iter().find(|p|p.pass_id==current.pass).context("real prospective p1 K1")?;let own=pass.distributions.iter().find(|p|p.obj_id()==current.b.obj_id()).context("actual planned K1 same full key")?;
 ensure!(own.obj_id()==current.b.obj_id()&&hash(own)?!=hash(&current.b)?&&pass.claim.dist_claim_id!=current.claim,"planned K1 not distinct generation/commitment at same physical ID");drop(payload);
 local.capture_own_body(body.clone(),case.h.state.witness_id()).await?;let request=tombstone::delete_first(&case).await?;
 let observed=Arc::new(repo::Repo::new(&case,&local));let p=Arc::new(PersistentObject{repo:observed.clone(),device_log_append_guard:case.h.p.device_log_append_guard.clone(),client_operation_guard:case.h.p.client_operation_guard.clone(),device_sync_guard:case.h.p.device_sync_guard.clone()});
 ensure!(Arc::ptr_eq(&p.device_log_append_guard,&case.h.p.device_log_append_guard)&&Arc::ptr_eq(&p.client_operation_guard,&case.h.p.client_operation_guard)&&Arc::ptr_eq(&p.device_sync_guard,&case.h.p.device_sync_guard),"SAME p_obj three guards changed");
 let wire=Arc::new(wire::Forward::new(&case,observed.clone(),&origin));let gateway=Arc::new(SyncGateway::new("actual-FIRST-uncut-C4".into(),p.clone(),wire.clone(),case.h.state.master.clone()));phase="FIRST_uncut_public_sync";
 let first=gateway.sync(case.h.state.b.user()).await;
 let technical=proof::technical(&case,&local,&observed,&wire,&original,&body).await;
 let all_attempts=proof::all_attempts(&case,&observed,&wire,&before,&claims,&planned);
 let (actual_packet,response_order,packet_matches)={
 let replies=wire.replies.lock().unwrap();let response=replies.iter().find(|r|matches!(&r.request,SyncRequest::Read(q)if matches!(&**q,ReadSyncRequest::SsRequest(_))));
 let actual_packet=response.and_then(|r|match &r.response{DataSyncResponse::Data(d)=>d.0.iter().find(|e|e.obj_id()==current.b.obj_id()).cloned().and_then(|e|e.shared_secret().ok()),_=>None});let response_order=response.map(|r|r.order);let packet_matches=actual_packet.as_ref()==Some(&current.b);
 (actual_packet,response_order,packet_matches)
 };
 let imported=response_order.is_some_and(|r|observed.writes.lock().unwrap().iter().any(|w|w.forward>r&&w.forward<w.done&&w.success&&w.exact&&w.event==current.b.clone().to_generic()));
 let unavailable=first.as_ref().err().is_some_and(|e|e.chain().any(|v|v.to_string()=="GuardedPreparationUnavailable"));
 eprintln!("C4_INCOMING actual_static_guard_unavailable={unavailable}");
 eprintln!("C4_INCOMING FIRST_whole_sync_ok={} FIRST_real_SS_response_K2={packet_matches} forwarded_successful_actual_K2_write={imported} technical_before_FIRST_SS_ok={} ALLattempts_noK1_ok={}",first.is_ok(),technical.is_ok(),all_attempts.is_ok());
 technical.as_ref().map_err(|_|anyhow::anyhow!("DEPENDENCY genuine technical settlement before FIRST SS failed"))?;all_attempts?;ensure!(packet_matches,"DEPENDENCY FIRST real addressed SS K2 response absent/different");
 // FIRST response cannot be consumed and hidden by a later retry. No allowed Err/cutpoint.
 ensure!(first.is_ok(),"SEMANTIC FIRST full sync blocked actual different current K2 at retired planned K1 fixed ID");ensure!(imported,"actual response K2 was not forwarded successfully into production storage");
 let compact=technical?;let actual_packet=actual_packet.unwrap();crypto::pairs(&case,&current,&actual_packet,"actual_FIRST_import")?;proof::stable(&case,&observed,&wire,&current,&compact,&request).await?;
 phase="strict_repeat";gateway.sync(case.h.state.b.user()).await?;proof::stable(&case,&observed,&wire,&current,&compact,&request).await?;
 let service=MetaClientService{data_transfer:Arc::new(MetaClientDataTransfer{dt:MetaClientRequestTransfer::new()}),sync_gateway:gateway.clone(),state_provider:Arc::new(MetaClientStateProvider::new()),p_obj:p.clone(),device_data:case.h.state.b.device(),master_key:case.h.state.master.clone()};
 for iteration in 0..2{phase="actual_GetState_TWICE";let state=service.get_app_state().await?;ensure!(matches!(state,ApplicationState::Vault(VaultFullInfo::Member(m))if m.member.member.user_data==case.h.state.b.user()&&m.member.vault.members().len()==3&&m.member.vault.secrets.contains(&current.pass)),"actual same-object GetState not Member3/current p1");proof::stable(&case,&observed,&wire,&current,&compact,&request).await?;let actual=observed.physical.find_one(current.b.obj_id()).await?.context("actual postGetState K2")?.shared_secret()?;crypto::pairs(&case,&current,&actual,"actual_repeat_GetState")?;eprintln!("C4_INCOMING LATE actual_GetState_iteration={} Member3=true immutable_compact_N=true current_K2=true p0_absent=true SAME_Pending=true",iteration+1);}
 proof::all_attempts(&case,&observed,&wire,&before,&claims,&planned)?;ensure!(wire.noops.lock().unwrap().len()==1,"repeat/GetState resent/changed settled N");Ok(())
 }.await;
    eprintln!("C4_INCOMING phase={phase} result_ok={}", result.is_ok());
    result
}
