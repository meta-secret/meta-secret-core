use super::{fixture::Recovery, fixtures::VALUE};
use anyhow::{Result, ensure};
use meta_secret_core::node::{
    common::model::secret::SsDistributionStatus,
    db::{
        actions::recover::RecoveryHandler, events::generic_log_event::ObjIdExtractor,
        repo::generic_db::*,
    },
};

#[tokio::test]
async fn no_local_two_remote_legacy_reconstruction_succeeds_with_no_proof() -> Result<()> {
    let mut r = Recovery::new().await?;
    r.claim
        .status
        .statuses
        .insert(r.h.target.device_id().clone(), SsDistributionStatus::Sent);
    r.save_claim(&r.claim).await?;
    r.packet(&r.h.target, &r.h.target).await?;
    let local = r.local().await?;
    r.h.p.repo.try_delete(local.obj_id()).await?;
    ensure!(
        r.context().await?.is_none(),
        "no-local current Liveboard does not invent generation proof"
    );
    let result = RecoveryHandler {
        p_obj: r.h.p.clone(),
    }
    .recover_by_claim_id_checked(r.h.approver.clone(), r.claim.id)
    .await?;
    ensure!(
        result.plaintext.text == VALUE && result.reveal_receipt.is_none(),
        "legacy no-local value survives optional proof absence"
    );
    Ok(())
}

#[tokio::test]
async fn all_consumed_extra_unapproved_packets_preserve_legacy_value_but_get_no_receipt()
-> Result<()> {
    let r = Recovery::new().await?;
    r.packet(&r.h.target, &r.h.target).await?;
    let result = RecoveryHandler {
        p_obj: r.h.p.clone(),
    }
    .recover_by_claim_id_checked(r.h.approver.clone(), r.claim.id)
    .await?;
    ensure!(
        result.plaintext.text == VALUE && result.reveal_receipt.is_none(),
        "complete actual consumed set cannot be narrowed to convenient approved subset"
    );
    Ok(())
}

#[tokio::test]
async fn raw_caller_pass_and_legacy_foreign_sender_behavior_is_not_reauthorized_by_metadata()
-> Result<()> {
    let r = Recovery::new().await?;
    let mut foreign = r.claim.clone();
    foreign.sender = r.h.target.device_id().clone();
    r.save_claim(&foreign).await?;
    // Raw recover historically accepts the caller's Pass and does not add the by-Claim sender gate.
    // Keep an actual addressed source under the changed sender descriptor too.
    let source = r.source().await?;
    let mut copied = source;
    if let meta_secret_core::node::db::events::shared_secret_event::SsWorkflowObject::Recovery(
        event,
    ) = &mut copied
    {
        let id = foreign
            .recovery_db_ids()
            .into_iter()
            .find(|id| id.distribution_id.receiver == *r.h.initiator.device_id())
            .unwrap();
        event.key = meta_secret_core::node::db::events::kv_log_event::KvKey::from(meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor::Recovery(id));
    }
    r.h.p.repo.save(copied).await?;
    let result = RecoveryHandler {
        p_obj: r.h.p.clone(),
    }
    .recover(r.h.approver.clone(), foreign.id, r.pass)
    .await?;
    ensure!(
        result.text == VALUE,
        "raw legacy caller policy remains unchanged"
    );
    Ok(())
}

#[tokio::test]
async fn oversized_optional_source_identity_preserves_legacy_plaintext_without_receipt()
-> Result<()> {
    let mut r = Recovery::new().await?;
    let mut old = r.claim.clone();
    old.status.statuses.insert(
        r.h.approver.device_id().clone(),
        SsDistributionStatus::Delivered,
    );
    r.save_claim(&old).await?;
    r.claim.id = meta_secret_core::node::common::model::secret::ClaimId::from(
        meta_secret_core::crypto::utils::Id48bit::from("x".repeat(1025)),
    );
    r.claim.dist_claim_id.id = r.claim.id.clone();
    r.save_claim(&r.claim).await?;
    r.packet(&r.h.initiator, &r.h.initiator).await?;
    let result = RecoveryHandler {
        p_obj: r.h.p.clone(),
    }
    .recover_by_claim_id_checked(r.h.approver.clone(), r.claim.id)
    .await?;
    ensure!(
        result.plaintext.text == VALUE && result.reveal_receipt.is_none(),
        "oversized proof cannot change legacy reconstruction"
    );
    Ok(())
}

#[tokio::test]
async fn raw_recover_keeps_distinct_supplied_pass_input_instead_of_deriving_claim_pass()
-> Result<()> {
    let mut r = Recovery::new().await?;
    let supplied = r.h.seed_secret("distinct-legacy-supplied-pass").await?;
    r.claim
        .status
        .statuses
        .insert(r.h.target.device_id().clone(), SsDistributionStatus::Sent);
    r.save_claim(&r.claim).await?;
    r.packet(&r.h.target, &r.h.target).await?;
    let local = r.h.p.find_tail_event(meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor::Distribution(
        meta_secret_core::node::common::model::secret::SsDistributionId {pass_id:supplied.clone(),receiver:r.h.approver.device_id().clone()})).await?.unwrap();
    r.h.p.repo.try_delete(local.obj_id()).await?;
    ensure!(
        supplied != r.pass,
        "test requires distinct available supplied Pass"
    );
    let result = RecoveryHandler {
        p_obj: r.h.p.clone(),
    }
    .recover(r.h.approver.clone(), r.claim.id, supplied)
    .await?;
    ensure!(
        result.text == VALUE,
        "raw caller-supplied Pass policy and input set are preserved"
    );
    Ok(())
}
