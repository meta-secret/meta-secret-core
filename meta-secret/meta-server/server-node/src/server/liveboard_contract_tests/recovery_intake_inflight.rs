use super::{
    recovery_inflight_fixture::Inflight,
    recovery_intake_faults::{Fault, IntakeFaultRepo, Phase},
    recovery_intake_fixture::assert_clean,
    *,
};
use meta_secret_core::node::db::repo::generic_db::FindOneQuery;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn recovery_inflight_actual_guarded_removal_canonical_failure_never_advances_original_raw()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(IntakeFaultRepo::new(inner.clone()));
    let h = Inflight::new(repo.clone()).await?;
    let before = inner.get_db().await;
    repo.arm(Phase::Canonical, Fault::Before);
    let result = send(&h.server, h.signed.clone()).await;
    assert!(result.is_err());
    assert_eq!(
        repo.hits.load(Ordering::SeqCst),
        1,
        "actual post-removal canonical fault reached"
    );
    assert!(
        inner.find_one(h.source.obj_id()).await?.is_none(),
        "RED: unchanged old-topology initial R advanced raw before canonical proof after ordinary guarded 3→2"
    );
    assert!(
        inner.get_db().await == before,
        "canonical-before fault must not mutate raw/topology/status/material"
    );
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), 0);
    Ok(())
}
#[tokio::test]
async fn recovery_inflight_actual_guarded_removal_no_fault_keeps_original_signed_source()
-> Result<()> {
    let repo = Arc::new(InMemKvLogEventRepo::default());
    let h = Inflight::new(repo.clone()).await?;
    let signed_bytes = canonical_json(&h.signed)?;
    send(&h.server, h.signed.clone()).await?;
    assert!(repo.find_one(h.source.obj_id()).await? == Some(h.source.clone().to_generic()));
    let log = h.log().await?;
    assert!(log.0.value.claims.get(&h.source.0.value.id) == Some(&h.source.0.value));
    assert!(log.0.value.pending_recovery_request_intake.is_none());
    assert_eq!(h.source.0.value.receivers.len(), 2);
    assert!(h.source.0.value.receivers.contains(h.base.c.device_id()));
    assert!(canonical_json(&h.signed)? == signed_bytes);
    h.signed.verify(&h.base.a.device().keys.dsa_pk)?;
    assert_clean(&h.server.handle_client_request(h.tail()).await?)?;
    assert!(send(&h.server, h.signed.clone()).await.is_err());
    Ok(())
}

#[tokio::test]
async fn same_a_pending_recovery_then_remove_c_keeps_exact_claim_and_future_local_remove_working()
-> Result<()> {
    use super::same_sender_pending_recovery_fixture::Scenario;
    use meta_secret_core::node::common::model::{
        liveboard::LiveboardOperationKind, secret::SsDistributionStatus,
    };

    let h = Scenario::new().await?;
    let recovery = h.a.service.recover_checked(h.pass.clone()).await?;
    assert_eq!(recovery.kind, LiveboardOperationKind::Recover);
    assert!(recovery.error_code.is_none());
    let claim = h
        .assert_admitted_recovery(
            recovery
                .claim_id
                .as_deref()
                .context("actual Recover ClaimId")?,
        )
        .await?;
    let removal =
        h.a.service
            .remove_member_checked(h.c.creds.device_id().to_string())
            .await?;
    let removal_id = h.assert_pending_remove(&removal).await?;
    h.assert_claim(&claim, SsDistributionStatus::Pending)
        .await?;
    let before_approval = h.wire.actions().len();
    h.approve(&removal_id, &removal).await?;
    h.assert_after_remote_remove(&claim, before_approval)
        .await?;
    h.assert_future_local_remove(&removal_id).await?;
    Ok(())
}
