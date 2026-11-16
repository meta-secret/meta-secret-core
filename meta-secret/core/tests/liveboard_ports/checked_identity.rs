use super::{fault_repo::Fault, fixtures::*};
use anyhow::{Context, Result};
use meta_secret_core::node::common::model::{
    liveboard::{LiveboardOperationKind, LiveboardOperationPhase, LiveboardOperationReceipt},
    meta_pass::{MetaPasswordId, PlainPassInfo},
    secret::{SecretDistributionType, SsClaim},
};
use meta_secret_core::node::db::{
    descriptors::{
        shared_secret_descriptor::SsDeviceLogDescriptor, vault_descriptor::DeviceLogDescriptor,
    },
    events::{
        shared_secret_event::SsDeviceLogObject,
        vault::{device_log_event::DeviceLogObject, vault_log_event::*},
    },
    objects::persistent_shared_secret::PersistentSharedSecret,
};
use std::sync::atomic::Ordering;

async fn added(h: &Harness) -> Result<Vec<MetaPasswordId>> {
    let events: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(h.approver.user_id()))
            .await?;
    Ok(events
        .into_iter()
        .filter_map(|e| match e.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(a)) => {
                Some(a.meta_pass_id)
            }
            _ => None,
        })
        .collect())
}
async fn recoveries(h: &Harness) -> Result<Vec<SsClaim>> {
    let events: Vec<SsDeviceLogObject> =
        h.p.get_object_events_from_beginning(SsDeviceLogDescriptor::from(
            h.approver.device_id().clone(),
        ))
        .await?;
    Ok(events
        .into_iter()
        .map(|e| e.to_distribution_request())
        .filter(|c| c.distribution_type == SecretDistributionType::Recover)
        .collect())
}
fn safe_receipt(receipt: &LiveboardOperationReceipt) -> Result<()> {
    let serialized = serde_json::to_string(receipt)?;
    for private in [
        VALUE,
        "PRIVATE-ERROR-CONTENT",
        "secretMessage",
        "shareBlocks",
        "signature",
        "keyManager",
    ] {
        assert!(
            !serialized.contains(private),
            "operation receipt leaked {private}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn checked_create_returns_the_actual_distributor_selected_identity_after_tombstoned_name_recreation()
-> Result<()> {
    let mut h = Harness::new(1).await?;
    let old = MetaPasswordId::build_from_str("same-name");
    h.vault.deleted_secrets.insert(old.clone());
    h.save_vault().await?;
    let receipt = h
        .service()
        .cluster_distribution_checked(PlainPassInfo {
            pass_id: old.clone(),
            pass: VALUE.into(),
        })
        .await
        .context("RED: checked Create must return the actual remapped distributor ID")?;
    let persisted = added(&h).await?;
    assert_eq!(persisted.len(), 1);
    assert_ne!(
        persisted[0].id, old.id,
        "recreation cannot resurrect tombstoned identity"
    );
    assert_eq!(persisted[0].name, old.name);
    assert_eq!(
        receipt.secret_id.as_deref(),
        Some(persisted[0].id_str().as_str())
    );
    assert_eq!(receipt.kind, LiveboardOperationKind::Create);
    assert!(receipt.claim_id.is_none());
    assert!(receipt.request_id.is_none());
    safe_receipt(&receipt)?;
    Ok(())
}

#[tokio::test]
async fn checked_create_failure_before_first_persistence_is_rejected_without_receipt_or_mutation()
-> Result<()> {
    let h = Harness::new(1).await?;
    let before = h.p.repo.get_db().await;
    h.p.repo.arm(Fault::BeforeDeviceLog);
    let result = h
        .service()
        .cluster_distribution_checked(PlainPassInfo::new("pre-persist".into(), VALUE.into()))
        .await;
    assert!(result.is_err());
    assert!(h.p.repo.get_db().await == before);
    assert_eq!(
        h.p.repo.hits.load(Ordering::SeqCst),
        1,
        "RED: checked Create must reach the real persistence fault"
    );
    Ok(())
}

#[tokio::test]
async fn checked_create_lost_first_save_reply_returns_the_actual_reserved_pass_not_a_name_guess()
-> Result<()> {
    create_fault(Fault::LostDeviceLogReply).await
}
#[tokio::test]
async fn checked_create_later_split_claim_failure_keeps_the_same_already_persisted_pass_identity()
-> Result<()> {
    create_fault(Fault::BeforeSsDeviceLog).await
}
#[tokio::test]
async fn checked_create_distribution_failure_keeps_the_same_opaque_identity_and_no_raw_error()
-> Result<()> {
    create_fault(Fault::BeforeDistribution).await
}
async fn create_fault(fault: Fault) -> Result<()> {
    let mut h = Harness::new(1).await?;
    let old = MetaPasswordId::build_from_str("post-persist-recreated");
    h.vault.deleted_secrets.insert(old.clone());
    h.save_vault().await?;
    h.p.repo.arm(fault);
    let receipt = h
        .service()
        .cluster_distribution_checked(PlainPassInfo {
            pass_id: old.clone(),
            pass: VALUE.into(),
        })
        .await
        .context("RED: actual post-persist identity must survive a partial Create failure")?;
    let persisted = added(&h).await?;
    assert_eq!(persisted.len(), 1);
    assert_ne!(persisted[0].id, old.id);
    assert_eq!(
        receipt.secret_id.as_deref(),
        Some(persisted[0].id_str().as_str())
    );
    assert!(matches!(
        receipt.phase,
        LiveboardOperationPhase::Queued | LiveboardOperationPhase::Unknown
    ));
    if receipt.phase == LiveboardOperationPhase::Unknown {
        assert!(receipt.error_code.is_some());
    }
    assert_eq!(h.p.repo.hits.load(Ordering::SeqCst), 1);
    safe_receipt(&receipt)?;
    // Observing/refreshing cannot submit a second named Create.
    h.service().get_app_state().await?;
    assert_eq!(added(&h).await?, persisted);
    Ok(())
}

#[tokio::test]
async fn checked_recover_returns_the_exact_new_claim_persisted_by_recovery_action() -> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("checked-new-recovery").await?;
    let receipt = h
        .service()
        .recover_checked(pass.clone())
        .await
        .context("RED: checked Recover must expose its actual saved Claim ID")?;
    let claims = recoveries(&h).await?;
    assert_eq!(claims.len(), 1);
    assert_eq!(receipt.kind, LiveboardOperationKind::Recover);
    assert_eq!(
        receipt.claim_id.as_deref(),
        Some(claims[0].id.wasm_id_str().as_str())
    );
    assert_eq!(receipt.secret_id.as_deref(), Some(pass.id_str().as_str()));
    assert_eq!(claims[0].sender, *h.approver.device_id());
    safe_receipt(&receipt)?;
    Ok(())
}

#[tokio::test]
async fn checked_recover_reuses_the_exact_active_claim_without_appending_another_random_claim()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("checked-reuse").await?;
    let vm = h.vault.clone().to_vault_member(
        meta_secret_core::node::common::model::user::common::UserDataMember::from(
            h.approver.user(),
        ),
    )?;
    let claim = vm.create_recovery_claim(pass.clone());
    let ss = PersistentSharedSecret::from(h.p.clone());
    ss.save_claim_in_ss_device_log(claim.clone()).await?;
    ss.save_ss_log_event(claim.clone()).await?;
    let before = recoveries(&h).await?;
    let receipt = h
        .service()
        .recover_checked(pass)
        .await
        .context("RED: existing active claim must return that same identity")?;
    assert_eq!(
        receipt.claim_id.as_deref(),
        Some(claim.id.wasm_id_str().as_str())
    );
    assert_eq!(recoveries(&h).await?, before);
    Ok(())
}

#[tokio::test]
async fn checked_recover_before_save_failure_is_rejected_with_no_actual_claim() -> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("recover-before-save").await?;
    let before = h.p.repo.get_db().await;
    h.p.repo.arm(Fault::BeforeSsDeviceLog);
    assert!(h.service().recover_checked(pass).await.is_err());
    assert!(h.p.repo.get_db().await == before);
    assert_eq!(
        h.p.repo.hits.load(Ordering::SeqCst),
        1,
        "RED: actual RecoveryAction save fault must be exercised"
    );
    assert!(recoveries(&h).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn checked_recover_saved_claim_with_lost_reply_still_returns_that_claim_as_unknown()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("recover-response-loss").await?;
    h.p.repo.arm(Fault::LostSsDeviceLogReply);
    let receipt = h
        .service()
        .recover_checked(pass)
        .await
        .context("RED: post-save response loss cannot discard actual Recovery identity")?;
    let claims = recoveries(&h).await?;
    assert_eq!(claims.len(), 1);
    assert_eq!(
        receipt.claim_id.as_deref(),
        Some(claims[0].id.wasm_id_str().as_str())
    );
    assert!(matches!(
        receipt.phase,
        LiveboardOperationPhase::Queued | LiveboardOperationPhase::Unknown
    ));
    assert_eq!(h.p.repo.hits.load(Ordering::SeqCst), 1);
    safe_receipt(&receipt)?;
    h.service().get_app_state().await?;
    assert_eq!(recoveries(&h).await?, claims);
    Ok(())
}

#[tokio::test]
async fn checked_recover_network_outage_after_save_reconciles_the_same_identity_without_resubmission()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("recover-network-outage").await?;
    h.protocol.fail_next_write.store(true, Ordering::SeqCst);
    let receipt = h
        .service()
        .recover_checked(pass)
        .await
        .context("RED: queued claim must keep its identity through network outage")?;
    let claims = recoveries(&h).await?;
    assert_eq!(claims.len(), 1);
    assert_eq!(
        receipt.claim_id.as_deref(),
        Some(claims[0].id.wasm_id_str().as_str())
    );
    assert!(matches!(
        receipt.phase,
        LiveboardOperationPhase::Queued | LiveboardOperationPhase::Unknown
    ));
    assert!(
        !h.protocol.fail_next_write.load(Ordering::SeqCst),
        "actual transport write fault must be reached"
    );
    safe_receipt(&receipt)?;
    h.service().get_app_state().await?;
    assert_eq!(recoveries(&h).await?, claims);
    Ok(())
}
