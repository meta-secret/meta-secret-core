use super::{
    correlation_faults::{CorrelationRepo, Fault, Step},
    correlation_support::{self, Resume},
    integration_binding_fixture::Seed,
    *,
};
use std::sync::atomic::Ordering;

pub(super) async fn interrupted<R: KvLogEventRepo>(
    inner: Arc<R>,
    count: usize,
    step: Step,
    fault: Fault,
) -> Result<Resume> {
    let repo = Arc::new(CorrelationRepo::new(inner));
    let h = Seed::new(repo.clone(), count).await?;
    let server = h.server().await?;
    let (source, signed, id) = h.deletion().await?;
    repo.arm(&source, &id, step, fault);
    let result = send(&server, signed.clone()).await;
    eprintln!(
        "[CORRELATION_PHASE] {step:?}/{fault:?} hits={} calls={:?}",
        repo.hits.load(Ordering::SeqCst),
        repo.calls.lock().unwrap()
    );
    assert!(
        repo.hits.load(Ordering::SeqCst) > 0,
        "RED: required automatic-Delete boundary was not executed"
    );
    let state = Resume {
        actor: h.actor.clone(),
        master: h.master.clone(),
        pass: h.pass.clone(),
        source,
        signed,
        id,
        workflows: h.workflows.clone(),
    };
    if fault == Fault::Occupied {
        assert!(
            result.is_err(),
            "different occupied target cannot be acknowledged"
        );
        assert!(repo.inner.find_one(state.source.obj_id()).await?.is_none());
        return Ok(state);
    }
    if result.is_ok() {
        // Lost response may be resolved by real equality readback; success must be complete.
        correlation_support::assert_complete(repo.inner.clone(), &state).await?;
    } else if step != Step::Raw || fault != Fault::LostReply {
        assert!(
            repo.inner.find_one(state.source.obj_id()).await?.is_none(),
            "unfinished phase cannot raw-ACK R"
        );
    }
    let log = PersistentVault::from(Arc::new(PersistentObject::new(repo.inner.clone())))
        .get_vault_log_artifact(state.actor.vault_name.clone())
        .await?;
    let has_p =
        log.0.value.requests.iter().any(
            |r| matches!(r, VaultActionRequestEvent::DeleteSecret(p) if p.request_id == state.id),
        ) || log
            .0
            .value
            .deletion_results
            .iter()
            .any(|r| r.request.request_id == state.id);
    if has_p {
        let source = correlation_support::source_ref(&log, &state.source, &state.id)?;
        correlation_support::assert_delete_ref(&source, &state.source, &state.signed, &state.id)?;
        if source["completionMode"] != "automaticComplete" {
            let wire = serde_json::to_value(&log.0.value)?;
            let phase = wire
                .get("automaticDeleteApplication")
                .context("RED: admitted incomplete automatic P has no durable phase")?;
            assert_eq!(phase["requestId"], state.id);
            assert_eq!(
                phase["sourceEventId"],
                serde_json::to_value(state.source.obj_id())?
            );
            assert_eq!(
                phase["sourceBodyCommitment"],
                correlation_support::commitment(&state.source)?
            );
            assert!(phase["phase"] == "prepared" || phase["phase"] == "purgePending");
        }
    }
    Ok(state) // All original server/backend/adapter handles close on return.
}
async fn fault(step: Step, kind: Fault) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let count = if step == Step::Admission { 1 } else { 2 };
    let state = interrupted(inner.clone(), count, step, kind).await?;
    if kind == Fault::Occupied {
        let before = inner.get_db().await;
        let server = ServerApp::new(inner.clone(), state.master.clone())?;
        assert!(
            server
                .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
                    meta_secret_core::node::api::ServerTailRequest {
                        sender: state.actor.user(),
                        producer_reference: Some(correlation_support::reference(
                            &state.source,
                            &state.id
                        )?)
                    }
                ))))
                .await
                .is_err(),
            "occupied inconsistent phase must remain unavailable on reopen"
        );
        assert!(
            inner.get_db().await == before,
            "do not overwrite conflicting history or rebase P"
        );
        return Ok(());
    }
    correlation_support::finish(inner, state).await
}
macro_rules! case {
    ($name:ident,$step:ident,$fault:ident) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            fault(Step::$step, Fault::$fault).await
        }
    };
}
case!(
    correlation_auto_admission_before_same_identity,
    Admission,
    Before
);
case!(correlation_auto_admission_drop_no_ack, Admission, Drop);
case!(
    correlation_auto_admission_occupied_conflict,
    Admission,
    Occupied
);
case!(
    correlation_auto_admission_lost_reply_same_identity,
    Admission,
    LostReply
);
case!(
    correlation_auto_admission_readback_uncertainty,
    Admission,
    Readback
);
// Target-Before is already covered by the unchanged original four backend + two phase cases.
case!(correlation_auto_target_drop_no_ack, Target, Drop);
case!(correlation_auto_target_occupied_conflict, Target, Occupied);
case!(
    correlation_auto_target_lost_reply_reuses_result,
    Target,
    LostReply
);
case!(
    correlation_auto_target_readback_before_terminal,
    Target,
    Readback
);
case!(
    correlation_auto_terminal_before_reconciles_existing_vault_result,
    Terminal,
    Before
);
case!(
    correlation_auto_terminal_drop_reconciles_same_result,
    Terminal,
    Drop
);
case!(
    correlation_auto_terminal_occupied_conflict,
    Terminal,
    Occupied
);
case!(
    correlation_auto_terminal_lost_reply_no_second_vault_apply,
    Terminal,
    LostReply
);
case!(
    correlation_auto_terminal_readback_no_raw_ack,
    Terminal,
    Readback
);
case!(
    correlation_auto_complete_before_keeps_purge_pending,
    Complete,
    Before
);
case!(correlation_auto_complete_drop_no_raw_ack, Complete, Drop);
case!(
    correlation_auto_complete_occupied_conflict,
    Complete,
    Occupied
);
case!(
    correlation_auto_complete_lost_reply_keeps_complete_marker,
    Complete,
    LostReply
);
case!(
    correlation_auto_complete_readback_no_ready_ack,
    Complete,
    Readback
);
case!(
    correlation_auto_raw_before_reuses_automatic_complete,
    Raw,
    Before
);
case!(
    correlation_auto_raw_lost_reply_reconciles_without_replay,
    Raw,
    LostReply
);
case!(
    correlation_auto_workflow_delete_before_retries_only_purge,
    Purge,
    Before
);
case!(
    correlation_auto_workflow_delete_drop_is_detected,
    Purge,
    Drop
);
case!(
    correlation_auto_workflow_delete_readback_is_required,
    Purge,
    Readback
);
