use super::*;
use meta_secret_core::node::{
    app::orchestrator::MetaOrchestrator, db::actions::recover::RecoveryAction, security::sign_event,
};

async fn import_b(case: &Case) -> Result<()> {
    let id = case
        .h
        .p
        .find_free_id_by_obj_desc(SsLogDescriptor::from(case.h.state.b.vault_name.clone()))
        .await?;
    let response = case
        .h
        .state
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
            SsRequest {
                sender: case.h.state.b.user(),
                ss_log: id,
            },
        ))))
        .await?
        .to_data()?;
    for event in response.0 {
        if case.h.p.repo.find_one(event.obj_id()).await?.is_some() {
            case.h.p.repo.replace(event).await?;
        } else {
            case.h.p.repo.save(event).await?;
        }
    }
    Ok(())
}
pub(super) async fn create(case: &Case) -> Result<Terminal> {
    let receipt = RecoveryAction::from(case.h.p.clone())
        .recovery_request_checked(case.h.state.b.clone(), case.h.state.passes[0].clone())
        .await?;
    ensure!(
        receipt.phase
            == meta_secret_core::node::common::model::liveboard::LiveboardOperationPhase::Queued,
        "actual checked Recovery receipt is not Queued"
    );
    let body: SsDeviceLogObject = case
        .h
        .p
        .find_tail_event(SsDeviceLogDescriptor::from(
            case.h.state.b.device_id().clone(),
        ))
        .await?
        .context("actual B Recover source")?;
    let claim = body.0.value.clone();
    let receivers: std::collections::HashSet<_> = claim.receivers.iter().cloned().collect();
    ensure!(
        receipt.claim_id.as_deref() == Some(claim.id.wasm_id_str().as_str())
            && claim.distribution_type == SecretDistributionType::Recover
            && claim.sender == *case.h.state.b.device_id()
            && receivers
                == [
                    case.h.state.a.device_id().clone(),
                    case.h.state.c.device_id().clone()
                ]
                .into_iter()
                .collect(),
        "terminal fixture must originate in actual checked B Recovery with A/C receivers"
    );
    let signed = sign_event(
        body.to_generic(),
        case.h.state.b.device_id().clone(),
        &case.h.state.b.device_creds.key_manager()?.dsa,
    )?;
    case.h
        .state
        .server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            signed,
        ))))
        .await?
        .ensure_success()?;
    import_b(case).await?;
    let pending = PersistentSharedSecret::from(case.h.p.clone())
        .get_ss_log_obj(case.h.state.b.vault_name.clone())
        .await?;
    ensure!(
        pending.claims.get(&claim.id).is_some_and(|x| x
            .status
            .statuses
            .values()
            .all(|s| *s == SsDistributionStatus::Pending)),
        "actual server-admitted pending Recovery was not imported"
    );
    // A's local INITIAL fixture consists only of copied ACTUAL current events. The decision below is real.
    let a_repo = Arc::new(InMemKvLogEventRepo::default());
    for event in case.h.p.repo.inner.inner.inner.get_db().await.into_values() {
        a_repo.save(event).await?;
    }
    let a = Arc::new(PersistentObject::new(a_repo));
    MetaOrchestrator {
        p_obj: a.clone(),
        user_creds: case.h.state.a.clone(),
    }
    .decline_recover(claim.id.clone())
    .await?;
    let recovery_id = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| id.distribution_id.receiver == *case.h.state.a.device_id())
        .context("actual A-addressed recovery descriptor")?;
    let workflow: SsWorkflowObject = a
        .find_tail_event(SsWorkflowDescriptor::Decline(recovery_id))
        .await?
        .context("actual A-created Decline workflow")?;
    let signed = sign_event(
        workflow.clone().to_generic(),
        case.h.state.a.device_id().clone(),
        &case.h.state.a.device_creds.key_manager()?.dsa,
    )?;
    let hash = proof::hash(&signed)?;
    case.h
        .state
        .server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            signed,
        ))))
        .await?
        .ensure_success()?;
    import_b(case).await?;
    let data = PersistentSharedSecret::from(case.h.p.clone())
        .get_ss_log_obj(case.h.state.b.vault_name.clone())
        .await?;
    let terminal = Terminal {
        claim: data
            .claims
            .get(&claim.id)
            .context("actual terminal Recovery claim")?
            .clone(),
        result: data
            .liveboard_recovery_results
            .get(&claim.id)
            .context("actual first-response result")?
            .clone(),
    };
    ensure!(
        terminal
            .claim
            .status
            .statuses
            .values()
            .all(|s| *s != SsDistributionStatus::Pending)
            && terminal.result.responder_id == *case.h.state.a.device_id()
            && terminal.result.outcome == DecisionOutcome::Declined
            && terminal
                .result
                .source
                .as_ref()
                .is_some_and(|s| s.workflow_artifact_id == workflow.obj_id()
                    && s.verified_envelope_hash == hash
                    && s.responder_id == *case.h.state.a.device_id()
                    && s.outcome == DecisionOutcome::Declined),
        "real signed A Decline did not produce terminal noPending claim and exact first-responder source proof"
    );
    ensure!(
        PersistentVault::from(case.h.p.clone())
            .get_vault(case.h.state.b.vault_name.clone())
            .await?
            .to_data()
            .members()
            .len()
            == 3,
        "Recovery companion changed membership before guarded prepare"
    );
    assert_final(case, &terminal).await?;
    eprintln!(
        "C2 claim companion actual B Recover+signed A Decline imported=true no_pending=true first_responder_A_sourceproof=true members=3"
    );
    Ok(terminal)
}
pub(super) async fn assert_final(case: &Case, expected: &Terminal) -> Result<()> {
    let local = PersistentSharedSecret::from(case.h.p.clone())
        .get_ss_log_obj(case.h.state.b.vault_name.clone())
        .await?;
    let server = PersistentSharedSecret::from(Arc::new(PersistentObject::new(
        case.h.state.server_repo.clone(),
    )))
    .get_ss_log_obj(case.h.state.b.vault_name.clone())
    .await?;
    ensure!(
        expected.preserved(&local) && expected.preserved(&server),
        "whole actual terminal Recovery claim/result differs locally or on actual server"
    );
    Ok(())
}
