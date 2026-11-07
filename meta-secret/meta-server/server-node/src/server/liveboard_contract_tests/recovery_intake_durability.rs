use super::{
    recovery_intake_faults::{Fault, IntakeFaultRepo, Phase},
    recovery_intake_fixture::{IntakeHarness, assert_clean},
    *,
};
use meta_secret_core::node::{
    common::model::secret::SsLogData, db::repo::generic_db::FindOneQuery,
};
use std::sync::atomic::Ordering;

async fn interrupted(phase: Phase, fault: Fault) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(IntakeFaultRepo::new(inner.clone()));
    let h = IntakeHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    repo.arm(phase, fault);
    let result = send(&server, h.signed.clone()).await;
    eprintln!(
        "[B2_PHASE] {phase:?}/{fault:?} hits={} read_hits={} calls={:?}",
        repo.hits.load(Ordering::SeqCst),
        repo.read_hits.load(Ordering::SeqCst),
        repo.calls.lock().unwrap()
    );
    assert!(
        result.is_err(),
        "RED: uncertain canonical/raw/cleanup persistence cannot acknowledge a healthy new request"
    );
    assert!(
        repo.hits.load(Ordering::SeqCst) > 0,
        "RED: required initial-request persistence/readback phase was not reached"
    );
    if phase == Phase::Canonical {
        assert!(
            inner.find_one(h.source.obj_id()).await?.is_none(),
            "RED: raw source advanced before canonical target was proved durable"
        );
    }
    let stored = inner.find_one(h.prepared.obj_id()).await?;
    if phase == Phase::Raw {
        assert!(
            matches!(&stored, Some(meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent::SsLog(log)) if log.0.value.pending_recovery_request_intake.is_some()),
            "RED: raw fault must leave original canonical claim and Prepared metadata in the same fixed record"
        );
    }
    assert_eq!(
        h.publisher.0.load(Ordering::SeqCst),
        0,
        "no healthy invalidation before all readbacks/cleanup complete"
    );
    repo.clear();
    drop(server);
    let reopened = h.server().await?;
    let response = reopened.handle_client_request(h.tail()).await?;
    assert_clean(&response)?;
    if stored.is_none() {
        assert!(inner.find_one(h.source.obj_id()).await?.is_none());
        send(&reopened, h.signed.clone()).await?; // No durable admission: ordinary original signed command remains valid.
    }
    h.assert_settled().await?;
    assert!(
        send(&reopened, h.signed.clone()).await.is_err(),
        "normal Event replay must stay rejected, not gain an exact acknowledgement exception"
    );
    let all = inner.get_db().await;
    assert_eq!(all.values().filter(|e| matches!(e, meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent::SsDeviceLog(log) if log.0.value.id == h.source.0.value.id)).count(), 1);
    Ok(())
}
macro_rules! case {
    ($name:ident, $phase:ident, $fault:ident) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            interrupted(Phase::$phase, Fault::$fault).await
        }
    };
}
case!(
    recovery_intake_canonical_before_prevents_raw_advance,
    Canonical,
    Before
);
case!(
    recovery_intake_canonical_lost_reply_prevents_raw_advance,
    Canonical,
    LostReply
);
case!(
    recovery_intake_canonical_silent_drop_prevents_raw_advance,
    Canonical,
    Drop
);
case!(
    recovery_intake_canonical_wrong_key_prevents_raw_advance,
    Canonical,
    WrongKey
);
case!(
    recovery_intake_canonical_readback_prevents_raw_advance,
    Canonical,
    Readback
);
case!(
    recovery_intake_raw_before_reopens_exact_prepared_source,
    Raw,
    Before
);
case!(
    recovery_intake_raw_lost_reply_reopens_exact_prepared_source,
    Raw,
    LostReply
);
case!(
    recovery_intake_raw_silent_drop_reopens_exact_prepared_source,
    Raw,
    Drop
);
case!(
    recovery_intake_raw_wrong_key_reopens_exact_prepared_source,
    Raw,
    WrongKey
);
case!(
    recovery_intake_raw_readback_reopens_exact_prepared_source,
    Raw,
    Readback
);
case!(
    recovery_intake_cleanup_before_replaces_same_target,
    Cleanup,
    Before
);
case!(
    recovery_intake_cleanup_lost_reply_does_not_rewrite_raw,
    Cleanup,
    LostReply
);
case!(
    recovery_intake_cleanup_silent_drop_does_not_publish_prepared,
    Cleanup,
    Drop
);
case!(
    recovery_intake_cleanup_wrong_key_does_not_acknowledge,
    Cleanup,
    WrongKey
);
case!(
    recovery_intake_cleanup_readback_does_not_acknowledge,
    Cleanup,
    Readback
);

#[tokio::test]
async fn recovery_intake_unique_current_prepared_owner_repairs_original_source_and_cleans_same_record()
-> Result<()> {
    let repo = Arc::new(InMemKvLogEventRepo::default());
    let h = IntakeHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    h.seed_prepared().await?;
    assert!(repo.find_one(h.source.obj_id()).await?.is_none());
    let response = server.handle_client_request(h.tail()).await?;
    h.assert_settled().await?;
    assert_clean(&response)?;
    assert!(send(&server, h.signed.clone()).await.is_err());
    Ok(())
}

#[tokio::test]
async fn recovery_intake_first_canonical_record_without_base_repairs_exact_original_source()
-> Result<()> {
    use meta_secret_core::node::db::repo::generic_db::DeleteCommand;
    let mut h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = h.server().await?;
    h.repo.try_delete(h.base_log.obj_id()).await?;
    h.prepared.0.key.obj_id = h.base_log.obj_id();
    let id = h.prepared.obj_id();
    let receipt = h
        .prepared
        .0
        .value
        .pending_recovery_request_intake
        .as_mut()
        .unwrap();
    receipt.base_ss_log_artifact_id = None;
    receipt.target_ss_log_artifact_id = id;
    h.seed_prepared().await?;
    assert_clean(&server.handle_client_request(h.tail()).await?)?;
    h.assert_settled().await?;
    Ok(())
}

#[tokio::test]
async fn recovery_intake_initial_success_control_retains_signed_bytes_and_clean_public_history()
-> Result<()> {
    let h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = h.server().await?;
    let before = canonical_json(&h.signed)?;
    send(&server, h.signed.clone()).await?;
    h.assert_settled().await?;
    let response = server
        .handle_client_request(h.ss(h.base_log.obj_id()))
        .await?;
    assert_clean(&response)?;
    assert!(canonical_json(&h.signed)? == before);
    h.signed.verify(&h.base.a.device().keys.dsa_pk)?;
    let old: SsLogData = serde_json::from_str("{\"claims\":{}}")?;
    assert!(old.pending_recovery_request_intake.is_none());
    assert!(!serde_json::to_string(&old)?.contains("pendingRecoveryRequestIntake"));
    Ok(())
}

#[tokio::test]
async fn recovery_intake_client_signed_canonical_metadata_is_rejected_without_mutation()
-> Result<()> {
    let repo = Arc::new(InMemKvLogEventRepo::default());
    let h = IntakeHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    let forged = sign_event(
        h.prepared.clone().to_generic(),
        h.base.a.device_id().clone(),
        &h.base.a.device_creds.key_manager()?.dsa,
    )?;
    let before = repo.get_db().await;
    assert!(send(&server, forged).await.is_err());
    assert!(repo.get_db().await == before);
    Ok(())
}
