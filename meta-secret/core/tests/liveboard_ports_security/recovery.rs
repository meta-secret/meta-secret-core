use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::common::model::{
    meta_pass::{MetaPasswordId, PlainPassInfo},
    secret::{SecretDistributionType, SsClaim, SsDistributionStatus},
    user::common::UserDataMember,
    vault::vault::VaultName,
};
use meta_secret_core::node::db::{
    actions::recover::RecoveryAction,
    descriptors::{
        object_descriptor::ToObjectDescriptor, shared_secret_descriptor::SsDeviceLogDescriptor,
    },
    events::{kv_log_event::*, shared_secret_event::SsDeviceLogObject},
    objects::{
        persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
    },
    repo::generic_db::*,
};
use std::sync::Arc;

fn claim(h: &Harness, pass: MetaPasswordId) -> Result<SsClaim> {
    Ok(h.vault
        .clone()
        .to_vault_member(UserDataMember::from(h.approver.user()))?
        .create_recovery_claim(pass))
}
async fn local(h: &Harness) -> Result<Vec<SsClaim>> {
    Ok(h.p
        .get_object_events_from_beginning::<SsDeviceLogDescriptor>(SsDeviceLogDescriptor::from(
            h.approver.device_id().clone(),
        ))
        .await?
        .into_iter()
        .map(|e: SsDeviceLogObject| e.to_distribution_request())
        .filter(|c| c.distribution_type == SecretDistributionType::Recover)
        .collect())
}

#[tokio::test]
async fn canonical_declined_and_done_override_original_local_pending_claim() -> Result<()> {
    for done in [false, true] {
        let mut h = Harness::new(3).await?;
        let pass = h.seed_secret("terminal-overrides-local").await?;
        let original = claim(&h, pass.clone())?;
        let ss = PersistentSharedSecret::from(h.p.clone());
        ss.save_claim_in_ss_device_log(original.clone()).await?;
        let mut canonical = original.clone();
        if done {
            canonical.status.statuses.insert(
                h.approver.device_id().clone(),
                SsDistributionStatus::Delivered,
            );
        } else {
            for status in canonical.status.statuses.values_mut() {
                *status = SsDistributionStatus::Declined;
            }
        }
        ss.save_ss_log_event(canonical).await?;
        let receipt = h.service().recover_checked(pass).await?;
        assert_ne!(
            receipt.claim_id.as_deref(),
            Some(original.id.wasm_id_str().as_str())
        );
        assert_eq!(local(&h).await?.len(), 2);
    }
    Ok(())
}

#[tokio::test]
async fn restart_reuses_actual_unprocessed_local_recovery_without_another_append() -> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("restart-pending-local").await?;
    let original = claim(&h, pass.clone())?;
    PersistentSharedSecret::from(h.p.clone())
        .save_claim_in_ss_device_log(original.clone())
        .await?;
    let before = h.p.repo.get_db().await;
    let reopened = RecoveryAction::from(Arc::new(PersistentObject::new(h.p.repo.clone())));
    let receipt = reopened
        .recovery_request_checked(h.approver.clone(), pass)
        .await?;
    assert_eq!(
        receipt.claim_id.as_deref(),
        Some(original.id.wasm_id_str().as_str())
    );
    assert!(
        h.p.repo.get_db().await == before,
        "reopen may not submit a new claim"
    );
    Ok(())
}

#[tokio::test]
async fn local_pending_lookup_excludes_wrong_vault_sender_kind_and_pass() -> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("only-exact-local-owner").await?;
    let correct = claim(&h, pass.clone())?;
    let ss = PersistentSharedSecret::from(h.p.clone());
    // All records are in the local sender's stream, so the payload predicates
    // must independently reject entries that cannot own this exact operation.
    let mut wrong_vault = claim(&h, pass.clone())?;
    wrong_vault.vault_name = VaultName("another-vault".into());
    let mut wrong_sender = claim(&h, pass.clone())?;
    wrong_sender.sender = h.initiator.device_id().clone();
    let mut wrong_kind = claim(&h, pass.clone())?;
    wrong_kind.distribution_type = SecretDistributionType::Split;
    let other_pass = PlainPassInfo::new("different-pass".into(), VALUE.into()).pass_id;
    let wrong_pass = claim(&h, other_pass)?;
    for wrong in [wrong_vault, wrong_sender, wrong_kind, wrong_pass] {
        // Use the real persisted stream structure without changing the production writer.
        let desc = SsDeviceLogDescriptor::from(h.approver.device_id().clone());
        let id = h.p.find_free_id_by_obj_desc(desc.clone()).await?;
        h.p.repo
            .save(SsDeviceLogObject(KvLogEvent {
                key: KvKey::artifact(desc.to_obj_desc(), id),
                value: wrong,
            }))
            .await?;
    }
    ss.save_claim_in_ss_device_log(correct.clone()).await?;
    let before = h.p.repo.get_db().await;
    let receipt = h.service().recover_checked(pass).await?;
    assert_eq!(
        receipt.claim_id.as_deref(),
        Some(correct.id.wasm_id_str().as_str())
    );
    assert!(h.p.repo.get_db().await == before);
    Ok(())
}

#[tokio::test]
async fn two_actual_local_pending_claims_fail_closed_without_random_claim_selection() -> Result<()>
{
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("ambiguous-local-pending").await?;
    let ss = PersistentSharedSecret::from(h.p.clone());
    ss.save_claim_in_ss_device_log(claim(&h, pass.clone())?)
        .await?;
    ss.save_claim_in_ss_device_log(claim(&h, pass.clone())?)
        .await?;
    let before = h.p.repo.get_db().await;
    let error = h
        .service()
        .recover_checked(pass)
        .await
        .err()
        .context("ambiguous local work must reject")?;
    assert_eq!(error.to_string(), "RecoverNotQueued");
    assert!(h.p.repo.get_db().await == before);
    assert_eq!(local(&h).await?.len(), 2);
    Ok(())
}
