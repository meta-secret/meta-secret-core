use super::prepared_faults::{FaultKind, Phase, PhaseRepo};
use super::*;
use meta_secret_core::node::common::model::secret::{SsClaim, SsDeclineData, SsDistributionStatus};
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use meta_secret_core::node::db::events::shared_secret_event::SsWorkflowObject;
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use meta_secret_core::node::db::repo::generic_db::FindOneQuery;
use std::sync::atomic::Ordering;

fn decline(actor: &UserCreds, claim: &SsClaim) -> Result<(SignedAction, ArtifactId)> {
    let id = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| &id.distribution_id.receiver == actor.device_id())
        .context("legitimate targeted receiver")?;
    let event = SsWorkflowObject::Decline(KvLogEvent {
        key: KvKey::from(SsWorkflowDescriptor::Decline(id)),
        value: SsDeclineData {
            vault_name: claim.vault_name.clone(),
            claim_id: claim.id.clone(),
            receiver_id: actor.device_id().clone(),
        },
    });
    let event_id = event.obj_id();
    Ok((
        sign_event(
            event.to_generic(),
            actor.device_id().clone(),
            &actor.device_creds.key_manager()?.dsa,
        )?,
        event_id,
    ))
}

#[tokio::test]
async fn liveboard_recovery_workflow_saved_before_sslog_failure_retries_same_verified_response()
-> Result<()> {
    let h = Harness::new().await?;
    let claim = super::recovery::pending_claim(&h).await?;
    let (original, source_id) = decline(&h.b, &claim)?;
    let repo = Arc::new(PhaseRepo::new(h.p_obj.repo.clone()));
    let server = ServerApp::new(repo.clone(), h.master_key.clone())?;
    server.init().await?;
    repo.arm(Phase::Claims, FaultKind::Save);
    assert!(send(&server, original.clone()).await.is_err());
    assert!(repo.hits.load(Ordering::SeqCst) > 0);
    assert!(
        h.p_obj.repo.find_one(source_id).await?.is_some(),
        "the real workflow was durably stored before the canonical SsLog fault"
    );
    let p_ss = PersistentSharedSecret::from(h.p_obj.clone());
    let partial = p_ss.get_ss_log_obj(h.a.vault_name.clone()).await?;
    assert!(
        partial.claims[&claim.id].status == claim.status
            && !partial.liveboard_recovery_results.contains_key(&claim.id),
        "status and winner metadata must not diverge on failed append"
    );
    repo.clear();
    let reopened = ServerApp::new(repo, h.master_key.clone())?;
    send(&reopened, original).await.context("RED: persisted recovery workflow replay was rejected while its canonical decision is still missing")?;
    let settled = p_ss.get_ss_log_obj(h.a.vault_name.clone()).await?;
    let first = settled
        .liveboard_recovery_results
        .get(&claim.id)
        .context("verified recovery winner was not committed")?
        .clone();
    assert!(first.responder_id == *h.b.device_id());
    assert!(
        claim
            .receivers
            .iter()
            .all(|receiver| settled.claims[&claim.id].status.get(receiver)
                == Some(&SsDistributionStatus::Declined))
    );
    let (late, _) = decline(&h.c, &claim)?;
    let _ = send(&reopened, late).await;
    let final_state = p_ss.get_ss_log_obj(h.a.vault_name.clone()).await?;
    assert!(
        final_state.liveboard_recovery_results.get(&claim.id) == Some(&first),
        "a later receiver cannot replace the first committed winner"
    );
    Ok(())
}
