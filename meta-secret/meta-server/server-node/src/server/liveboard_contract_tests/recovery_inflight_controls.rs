use super::{
    recovery_inflight_fixture::Inflight,
    recovery_intake_faults::{Fault, IntakeFaultRepo, Phase},
    *,
};
use meta_secret_core::node::{
    app::orchestrator::MetaOrchestrator,
    common::model::vault::vault::VaultMember,
    db::{
        actions::{recover::RecoveryAction, sign_up::join::JoinActionUpdate},
        descriptors::shared_secret_descriptor::SsDeviceLogDescriptor,
        events::shared_secret_event::SsDeviceLogObject,
        objects::{
            persistent_device_log::PersistentDeviceLog,
            persistent_shared_secret::PersistentSharedSecret,
        },
        repo::generic_db::FindOneQuery,
    },
};
use std::sync::atomic::Ordering;

#[tokio::test]
async fn recovery_inflight_prior_ordinary_join_protects_actual_stored_original_subset() -> Result<()>
{
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(IntakeFaultRepo::new(inner.clone()));
    let h = Inflight::new(repo.clone()).await?;
    h.pull_canonical().await?;
    RecoveryAction::from(h.local.clone())
        .recovery_request_checked(h.base.b.clone(), h.pass.clone())
        .await?;
    let original = PersistentSharedSecret::from(h.local.clone())
        .find_ss_device_log_tail_event(h.base.b.device_id().clone())
        .await?
        .context("real Core-stored subset before actual join")?;
    assert_eq!(
        original.0.value.receivers,
        vec![h.base.a.device_id().clone()]
    );
    let signed = sign_event(
        original.clone().to_generic(),
        h.base.b.device_id().clone(),
        &h.base.b.device_creds.key_manager()?.dsa,
    )?;
    let bytes = canonical_json(&signed)?;
    let devices = PersistentDeviceLog::from(h.local.clone());
    devices.save_join_request(&h.base.c.user()).await?;
    let join = devices
        .latest_join_request(&h.base.c.user())
        .await?
        .context("Core generated genuine new Join request")?;
    let event: DeviceLogObject = h
        .local
        .find_tail_event(DeviceLogDescriptor::from(h.base.c.user_id()))
        .await?
        .context("stored C Join")?;
    send(
        &h.server,
        sign_event(
            event.to_generic(),
            h.base.c.device_id().clone(),
            &h.base.c.device_creds.key_manager()?.dsa,
        )?,
    )
    .await?;
    h.pull_canonical().await?;
    MetaOrchestrator {
        p_obj: h.local.clone(),
        user_creds: h.base.a.clone(),
    }
    .update_membership(join, JoinActionUpdate::Accept)
    .await?;
    let update: DeviceLogObject = h
        .local
        .find_tail_event(DeviceLogDescriptor::from(h.base.a.user_id()))
        .await?
        .context("actual Core Join approval with resharing")?;
    send(
        &h.server,
        sign_event(
            update.to_generic(),
            h.base.a.device_id().clone(),
            &h.base.a.device_creds.key_manager()?.dsa,
        )?,
    )
    .await?;
    let joined = PersistentVault::from(Arc::new(PersistentObject::new(inner.clone())))
        .get_vault(h.base.a.vault_name.clone())
        .await?;
    assert_eq!(joined.0.value.members().len(), 3);
    assert!(joined.0.value.is_member(h.base.c.device_id()));
    assert!(inner.find_one(original.obj_id()).await?.is_none());
    assert!(
        !h.log()
            .await?
            .0
            .value
            .claims
            .contains_key(&original.0.value.id)
    );
    h.publisher.0.store(0, Ordering::SeqCst);
    repo.arm(Phase::Canonical, Fault::Before);
    assert!(send(&h.server, signed.clone()).await.is_err());
    assert!(repo.hits.load(Ordering::SeqCst) > 0);
    assert!(
        inner.find_one(original.obj_id()).await?.is_none(),
        "RED: original subset source advanced raw after actual ordinary Join"
    );
    assert!(canonical_json(&signed)? == bytes);
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), 0);
    Ok(())
}
#[tokio::test]
async fn recovery_inflight_existing_pending_and_different_active_claim_keep_authoritative_legacy_path()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(IntakeFaultRepo::new(inner.clone()));
    let h = Inflight::new(repo.clone()).await?;
    send(&h.server, h.signed.clone()).await?;
    let first = h.log().await?;
    let mut snapshot = h.source.clone();
    snapshot.0.key = snapshot.0.key.next();
    repo.arm(Phase::Cleanup, Fault::Before);
    send(
        &h.server,
        sign_event(
            snapshot.clone().to_generic(),
            h.base.a.device_id().clone(),
            &h.base.a.device_creds.key_manager()?.dsa,
        )?,
    )
    .await?;
    assert_eq!(
        repo.hits.load(Ordering::SeqCst),
        0,
        "same-ID canonical merge must not get a new initial receipt"
    );
    assert!(h.log().await?.0.value.claims == first.0.value.claims);
    let distinct = VaultMember {
        member: member(&h.base.a),
        vault: h.admission.0.value.clone(),
    }
    .create_recovery_claim(h.pass.clone());
    assert_ne!(distinct.id, h.source.0.value.id);
    snapshot.0.key = snapshot.0.key.next();
    snapshot.0.value = distinct;
    let before = inner.get_db().await;
    send(
        &h.server,
        sign_event(
            snapshot.to_generic(),
            h.base.a.device_id().clone(),
            &h.base.a.device_creds.key_manager()?.dsa,
        )?,
    )
    .await?;
    assert!(
        inner.get_db().await == before,
        "different active same-owner/Pass candidate remains a no-op, not a new Claim/receipt"
    );
    assert!(
        h.log()
            .await?
            .0
            .value
            .pending_recovery_request_intake
            .is_none()
    );
    Ok(())
}
#[tokio::test]
async fn recovery_inflight_removed_requester_with_core_initial_shape_is_rejected_before_any_write()
-> Result<()> {
    let h = Inflight::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let claim = VaultMember {
        member: member(&h.base.c),
        vault: h.pre_vault.0.value.clone(),
    }
    .create_recovery_claim(h.pass.clone());
    let source = SsDeviceLogObject(KvLogEvent {
        key: KvKey::from(SsDeviceLogDescriptor::from(h.base.c.device_id().clone())),
        value: claim,
    });
    let signed = sign_event(
        source.to_generic(),
        h.base.c.device_id().clone(),
        &h.base.c.device_creds.key_manager()?.dsa,
    )?;
    signed.verify(&h.base.c.device().keys.dsa_pk)?;
    let before = h.repo.get_db().await;
    assert!(send(&h.server, signed).await.is_err());
    assert!(h.repo.get_db().await == before);
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), 0);
    Ok(())
}
#[tokio::test]
async fn recovery_inflight_legacy_accepted_duplicate_receiver_snapshot_gains_no_new_intake_authority()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(IntakeFaultRepo::new(inner));
    let h = Inflight::new(repo.clone()).await?;
    let mut source = h.source.clone();
    source
        .0
        .value
        .receivers
        .push(source.0.value.receivers[0].clone());
    repo.arm(Phase::Cleanup, Fault::Before);
    send(
        &h.server,
        sign_event(
            source.clone().to_generic(),
            h.base.a.device_id().clone(),
            &h.base.a.device_creds.key_manager()?.dsa,
        )?,
    )
    .await?;
    assert_eq!(repo.hits.load(Ordering::SeqCst), 0);
    let calls = repo.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![Phase::Raw, Phase::Canonical],
        "this control preserves actual legacy routing; it does not invent new malformed rejection policy"
    );
    let log = h.log().await?;
    assert!(log.0.value.pending_recovery_request_intake.is_none());
    assert!(log.0.value.claims.get(&source.0.value.id) == Some(&source.0.value));
    Ok(())
}
