//! INITIAL3 is a fixture; both subsequent removals use real signed ServerApp actions.
use super::*;
#[path = "singleton_negative.rs"]
mod singleton_negative;
#[path = "singleton_proof.rs"]
mod singleton_proof;
use meta_secret_core::node::common::model::{
    liveboard_producer::{
        ProducerCanonicalState, ProducerDecisionStage, ProducerMaterialStage, ProducerPhase,
        ProducerSourceRef,
    },
    user::common::{UserDataOutsiderStatus, UserMembership},
};

async fn published_owner() -> Result<harness::Harness> {
    // Logical A=state.b (owner/approver), B=state.a (requester), C=state.c.
    // actor::case imports only actual addressed data into an empty owner repository.
    let (case, local) = actor::case().await?;
    let (receipt, frozen) = case.prepare(true).await?;
    private_crypto(&case.h.state, &case.h.witness().await?)?;
    let repo = Arc::new(repo::Repo::new(&case, &local));
    let owners = &case.h.p;
    let p = Arc::new(PersistentObject {
        repo: repo.clone(),
        device_log_append_guard: owners.device_log_append_guard.clone(),
        client_operation_guard: owners.client_operation_guard.clone(),
        device_sync_guard: owners.device_sync_guard.clone(),
    });
    ensure!(
        Arc::ptr_eq(&p.device_sync_guard, &owners.device_sync_guard)
            && Arc::ptr_eq(&p.client_operation_guard, &owners.client_operation_guard)
            && Arc::ptr_eq(&p.device_log_append_guard, &owners.device_log_append_guard),
        "SAME owners required"
    );
    let wire = Arc::new(wire::Forward::new(&case, repo.clone()));
    let gateway = Arc::new(SyncGateway::new(
        "singleton-lifetime".into(),
        p.clone(),
        wire.clone(),
        case.h.state.master.clone(),
    ));
    let h = harness::Harness {
        case,
        receipt,
        local,
        repo,
        wire,
        p,
        gateway,
    };
    h.sync()
        .await
        .context("SETUP real own Applied3to2/publication")?;
    publication::common(&publication::measure(&h.case).await?)?;
    publication::published(&h.case, &frozen).await?;
    proof::published(&h, &frozen).await?;
    ensure!(
        h.case.observations.fault_hits.load(Ordering::SeqCst) == 0
            && h.local.hits.load(Ordering::SeqCst) == 0
            && h.case.observations.noops.load(Ordering::SeqCst) == 0,
        "SETUP must be fault-free Business"
    );
    Ok(h)
}

async fn ready_show(h: &harness::Harness) -> Result<()> {
    let service = Arc::new(h.service());
    let state = service.get_app_state().await?;
    let ApplicationState::Vault(VaultFullInfo::Member(member)) = state else {
        anyhow::bail!("SETUP actual Member2 required")
    };
    let a = &h.case.h.state.b;
    let b = &h.case.h.state.a;
    let members = member.member.vault.members();
    ensure!(
        members.len() == 2
            && members.iter().any(|m| m.user_data == a.user())
            && members.iter().any(|m| m.user_data == b.user()),
        "SETUP exact A+B survivors"
    );
    let pass = &h.case.h.state.passes[0];
    let board = member
        .liveboard_state
        .context("SETUP actual Core metadata")?;
    ensure!(
        board.secrets.iter().any(|s| s.secret_id == pass.id_str()
            && s.primary_action == "showLocal"
            && s.readiness == "ready"),
        "SETUP Core retained Pass not ready/showLocal"
    );
    let manager = meta_secret_wasm::app_manager::ApplicationManager::new(
        h.wire.clone(),
        h.gateway.clone(),
        service,
        h.case.h.state.master.clone(),
    );
    ensure!(
        manager.show_local_secret(pass.clone()).await?.text == "C1_PRIVATE_FIXTURE_FIRST",
        "SETUP actual retained plaintext mismatch"
    );
    eprintln!(
        "SINGLETON SETUP Core_ready=true actual_public_Show_RAM_match=true INITIAL3_fixture=true"
    );
    Ok(())
}

#[tokio::test]
async fn owned_applied_3to2_then_local_2to1_reaches_final_observation_without_fault() -> Result<()>
{
    let h = published_owner().await?;
    ready_show(&h).await?;
    let old = h.case.h.witness().await?;
    ensure!(
        old.0.value.phase == GuardedPreparationPhase::AcceptedPublished
            && old.0.value.payload.is_none()
            && old.0.value.transport_noop.is_none(),
        "SETUP original Published/None absent"
    );
    let (before_vault, before_log) = super::super::canonical(&h.case).await?;
    let results = &before_log.0.value.liveboard_removal_results;
    let old_result = results
        .iter()
        .find(|r| r.request_id == h.case.h.state.uuid)
        .context("SETUP original Applied result")?
        .clone();
    let old_value = &old.0.value;
    let old_origin = old_value.origin.as_ref().context("SETUP origin")?;
    let signature = old_origin
        .business_signature
        .as_ref()
        .context("SETUP signature")?;
    let original_source = old_result.source.as_ref().context("SETUP source")?;
    ensure!(
        old_result.application_state == RemovalApplicationState::Applied
            && old_result.outcome == DecisionOutcome::Accepted
            && original_source.event_id == old_origin.event_id
            && original_source.signer == *h.case.h.state.b.device_id()
            && original_source.signed_action_hash == signature.signed_action_hash,
        "SETUP exact old Applied source missing"
    );
    let prefix = PersistentDeviceLog::from(h.p.clone())
        .find_tail_id(&h.case.h.state.b.user_id())
        .await?
        .context("SETUP actual A stream prefix")?;
    let start = h.repo.tick();
    // Direct public service producer, no FIFO shortcut, no background race or injected fault.
    let receipt = h
        .service()
        .remove_member_checked(h.case.h.state.a.device_id().to_string())
        .await?;
    let returned = h.repo.tick();
    let (u_source, material_ready) = match &receipt.decision_stage {
        Some(ProducerDecisionStage::Attempted { source, .. }) => (
            source,
            receipt.material_stage == Some(ProducerMaterialStage::Ready),
        ),
        _ => anyhow::bail!("SEMANTIC actual independent U not attempted"),
    };
    let (r, _, _) = singleton_proof::actual_source(&h, &receipt.request_stage.source).await?;
    let (u, signed_u, u_sent) = singleton_proof::actual_source(&h, u_source).await?;
    ensure!(
        r.obj_id() == prefix.next() && u.obj_id() == r.obj_id().next(),
        "actual R/U fullID sequence changed"
    );
    let (
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request)),
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)),
    ) = (&r.0.value, &u.0.value)
    else {
        anyhow::bail!("actual R/U removal types required")
    };
    let a = &h.case.h.state.b;
    let b = &h.case.h.state.a;
    ensure!(request.sender.user_data == a.user() && request.target.user_data == b.user()
        && update.request == *request && update.sender.user_data == a.user()
        && update.target == request.target && update.decision == meta_secret_core::node::db::events::vault::vault_log_event::RemoveMemberDecision::Accepted,
        "actual R/U role/request binding differs");
    let (vault, log) = super::super::canonical(&h.case).await?;
    let request_id =
        singleton_proof::later_uuid(&h, &before_vault, &vault, &log, &r, &u, &signed_u).await?;
    let result = log
        .0
        .value
        .liveboard_removal_results
        .iter()
        .find(|r| r.request_id == request_id)
        .context("actual Prepared UUID Applied result missing")?;
    ensure!(
        result.request_id != old_result.request_id
            && result.source.is_none()
            && result.requester_id == *a.device_id()
            && result.target_id == *b.device_id()
            && result.responder_id == *a.device_id()
            && result.outcome == DecisionOutcome::Accepted
            && result.application_state == RemovalApplicationState::Applied
            && log.0.value.liveboard_removal_results.contains(&old_result),
        "actual later Applied/source lineage or retained old result differs"
    );
    ensure!(
        vault.obj_id().fqdn == before_vault.obj_id().fqdn
            && vault.obj_id().id.curr > before_vault.obj_id().id.curr
            && vault.0.value.members().len() == 1
            && vault.0.value.members()[0].user_data == a.user()
            && matches!(vault.0.value.find_user(b.device_id()), Some(UserMembership::Outsider(o)) if o.user_data == b.user() && o.status == UserDataOutsiderStatus::Declined),
        "SEMANTIC real canonical singleton A/removed B absent"
    );
    let mut public_log = log.clone();
    public_log.0.value = public_log.0.value.without_private_producer_metadata();
    let persistent = PersistentVault::from(h.p.clone());
    let local_log = persistent
        .get_vault_log_artifact(a.vault_name.clone())
        .await?;
    ensure!(
        persistent.get_vault(a.vault_name.clone()).await? == vault && local_log == public_log,
        "SEMANTIC actual local canonical singleton import missing"
    );
    let admission_bound = {
        let writes = h.repo.writes.lock().unwrap();
        let reads = h.repo.reads.lock().unwrap();
        writes.iter().any(|w| {
            w.forward > start
                && w.done < u_sent
                && w.success
                && w.exact
                && matches!(&w.event, GenericKvLogEvent::LocalRemovalEnvelope(e)
                if e.0.value.producer_context.as_ref().is_some_and(|c| c.owner == a.user_id()
                    && c.admission_vault == before_vault.obj_id() && c.target == *b.device_id()
                    && c.request_stage.source == receipt.request_stage.source
                    && c.decision_stage.as_ref().is_some_and(|d| d.source == *u_source)))
                && reads.iter().any(|r| {
                    r.order > w.done && r.order < u_sent && r.event.as_ref() == Some(&w.event)
                })
        })
    };
    ensure!(
        admission_bound,
        "actual R/U not bound to real A+B admission Vault"
    );
    let trace_bound = {
        let reads = h.repo.reads.lock().unwrap();
        let raw_vault = vault.clone().to_generic();
        let writes = h.repo.writes.lock().unwrap();
        let singleton_import = writes
            .iter()
            .find(|w| {
                w.forward > u_sent
                    && w.done < returned
                    && w.success
                    && w.exact
                    && w.event == raw_vault
            })
            .context("actual successful production singleton import")?
            .done;
        [
            old.clone().to_generic(),
            public_log.clone().to_generic(),
            vault.clone().to_generic(),
        ]
        .iter()
        .all(|expected| {
            reads.iter().any(|w| {
                w.order > singleton_import
                    && w.order < returned
                    && w.event.as_ref() == Some(expected)
            })
        })
    };
    ensure!(
        trace_bound && h.repo.trace_valid(),
        "actual original witness/log/singleton production read trace absent"
    );
    let actual_witness = h.case.h.witness().await?;
    let mut terminal = old.clone();
    terminal.0.value.phase = GuardedPreparationPhase::BlockedRetiredCommand;
    ensure!(
        actual_witness == old || actual_witness == terminal,
        "original ownership/compact/history was rewritten"
    );
    let final_observations = {
        let calls = h.wire.calls.lock().unwrap();
        calls.iter().filter(|c| c.order > start && c.order < returned && matches!(&c.request,
            SyncRequest::Read(q) if matches!(&**q, ReadSyncRequest::ServerTail(t) if t.producer_reference.as_ref().is_some_and(|r|
                r.request_stage == receipt.request_stage.source && r.decision_stage.as_ref() == Some(u_source)))))
            .count()
    };
    let observed_reply = h.wire.replies.lock().unwrap().iter().any(|r| r.order > u_sent && r.order < returned
        && matches!(&r.response, DataSyncResponse::ServerTailResponse(t) if t.producer_observation.as_ref().is_some_and(|o|
            o.request_stage.source == receipt.request_stage.source && o.decision_stage.as_ref().is_some_and(|d| d.source == *u_source)
                && o.canonical_state == ProducerCanonicalState::Applied && o.request_id.as_ref() == Some(&result.request_id))));
    // Separate healthy public retry exposes the fixed local proof error, not its text/payload.
    let repeat_marker = (
        h.repo.calls.lock().unwrap().len(),
        h.repo.writes.lock().unwrap().len(),
        h.wire.calls.lock().unwrap().len(),
    );
    let repeated = h.sync().await;
    let unavailable = repeated.as_ref().err().is_some_and(|e| {
        e.chain()
            .any(|x| x.to_string() == "GuardedPreparationUnavailable")
    });
    ensure!(
        h.case.observations.fault_hits.load(Ordering::SeqCst) == 0
            && h.local.hits.load(Ordering::SeqCst) == 0
            && h.wire.noops.lock().unwrap().is_empty(),
        "healthy regression accidentally injected a fault or Noop"
    );
    eprintln!(
        "SINGLETON actual_R_U_ACK=true server_local_Applied1=true old_Applied_source_retained=true trace_bound={trace_bound} material_ready={material_ready} final_reference_observations={final_observations} actual_guard_unavailable={unavailable}"
    );
    // Unknown is the semantic RED; no setup/fault/allowed-error branch satisfies success.
    ensure!(
        receipt.phase == ProducerPhase::Queued
            && receipt.canonical_state == ProducerCanonicalState::Applied
            && material_ready
            && receipt.request_id.as_ref() == Some(&result.request_id)
            && final_observations == 1
            && observed_reply,
        "SEMANTIC healthy Applied3to2 then local2to1 must reach final producer observation and ready Applied receipt"
    );
    singleton_proof::terminal_before_clear(&h, &terminal, &r, &u, returned).await?;
    repeated.context("SEMANTIC healthy singleton repeat sync")?;
    singleton_proof::ready_singleton(&h, &old, &terminal, &r, &u, repeat_marker).await
}
