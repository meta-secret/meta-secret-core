use super::{
    correlation_support as proof,
    integration_binding_fixture::{FaultRepo, Seed},
    *,
};
use meta_secret_core::node::{
    common::model::liveboard_producer::*, db::repo::generic_db::FindOneQuery,
};

#[tokio::test]
async fn correlation_verified_delete_ref_binds_actual_uuid_actor_subject_full_source_and_envelope()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(FaultRepo::new(inner.clone()));
    let h = Seed::new(repo.clone(), 3).await?;
    let server = h.server().await?;
    let (source, signed, id) = h.deletion().await?;
    repo.before_raw();
    assert!(send(&server, signed.clone()).await.is_err());
    let log = h.log().await?;
    assert!(
        log.0
            .value
            .requests
            .iter()
            .any(|p| matches!(p, VaultActionRequestEvent::DeleteSecret(p) if p.request_id == id))
    );
    let stored = proof::source_ref(&log, &source, &id)?;
    proof::assert_delete_ref(&stored, &source, &signed, &id)?;
    assert_eq!(stored["completionMode"], "remotePending");
    let roundtrip: VaultLogObject = serde_json::from_value(serde_json::to_value(&log)?)?;
    assert!(
        roundtrip == log,
        "private metadata persists through actual storage codec shape"
    );
    assert!(inner.find_one(source.obj_id()).await?.is_none());
    // Negative current-authority fixture: a newer canonical context no longer has A as member.
    // This is not a claimed ordinary removal or a new automatic permission.
    let pv = PersistentVault::from(Arc::new(PersistentObject::new(inner.clone())));
    let mut context = pv.get_vault(h.actor.vault_name.clone()).await?;
    context.0.key.obj_id = context.obj_id().next();
    context.0.value =
        context
            .0
            .value
            .update_membership(UserMembership::Outsider(UserDataOutsider {
                user_data: h.actor.user(),
                status: UserDataOutsiderStatus::Declined,
            }));
    inner.save(context).await?;
    let before = inner.get_db().await;
    assert!(
        send(&server, signed).await.is_err(),
        "old verified ref is not current raw-intake authority"
    );
    assert!(inner.get_db().await == before);
    Ok(())
}
#[tokio::test]
async fn correlation_known_admitted_source_full_body_conflict_never_reapplies_as_obsolete_request()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(FaultRepo::new(inner.clone()));
    let h = Seed::new(repo.clone(), 3).await?;
    let server = h.server().await?;
    let (mut source, signed, _) = h.deletion().await?;
    repo.before_raw();
    assert!(send(&server, signed).await.is_err());
    let VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(ref mut p)) =
        source.0.value
    else {
        unreachable!()
    };
    p.members.reverse(); // Same actual source key/UUID, different fully signed body; legacy stale handling must not run.
    let changed = sign_event(
        source.to_generic(),
        h.actor.device_id().clone(),
        &h.actor.device_creds.key_manager()?.dsa,
    )?;
    changed.verify(&h.actor.device().keys.dsa_pk)?;
    let before = inner.get_db().await;
    assert!(
        send(&server, changed).await.is_err(),
        "RED: exact admitted R cannot be re-applied as a different obsolete body"
    );
    assert!(
        inner.get_db().await == before,
        "changed body must not replace Pending/winner/UUID or ACK raw"
    );
    Ok(())
}
#[tokio::test]
async fn correlation_query_requires_full_source_uuid_kind_actor_vault_agreement_not_target_match()
-> Result<()> {
    let h = Seed::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    let server = h.server().await?;
    let (source, signed, id) = h.deletion().await?;
    send(&server, signed).await?;
    let (_, _, another_actual_uuid) = h.deletion().await?;
    let vault = PersistentVault::from(h.p.clone())
        .get_vault(h.actor.vault_name.clone())
        .await?;
    let peer = vault
        .0
        .value
        .members()
        .into_iter()
        .find(|u| u.user_data.device.device_id != *h.actor.device_id())
        .context("actual other member")?
        .user_data;
    for variant in 0..6 {
        let mut input = proof::reference(&source, &id)?;
        let mut sender = h.actor.user();
        match variant {
            0 => input.request_id = Some(another_actual_uuid.clone()),
            1 => input.request_stage.body_commitment = "00".repeat(32),
            2 => input.request_stage.event_id = source.obj_id().next(),
            3 => input.kind = ProducerKind::RemoveDevice,
            4 => sender = peer.clone(),
            _ => sender.vault_name = "unrelated-claimed-context".into(),
        }
        let before = h.repo.get_db().await;
        let response = proof::observe(&server, sender, input).await;
        assert!(
            h.repo.get_db().await == before,
            "unsigned mismatched query cannot mint proof or command"
        );
        if let Ok(response) = response {
            let observed = response
                .producer_observation
                .context("RED: no typed unknown/conflict source observation")?;
            assert_eq!(observed.canonical_state, ProducerCanonicalState::Unknown);
            assert!(!matches!(
                observed.request_stage.observation,
                ProducerSourceObservation::Admitted
                    | ProducerSourceObservation::RawAcknowledged
                    | ProducerSourceObservation::Redacted
            ));
        }
    }
    Ok(())
}
#[tokio::test]
async fn correlation_unproven_legacy_remove_stream_and_historical_pending_never_mint_verified_origin()
-> Result<()> {
    for historical in [false, true] {
        let h = Harness::new().await?;
        let source = if historical {
            h.historical_pending().await?;
            h.refresh().await?;
            next_event(
                &h.p_obj,
                &h.a,
                VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.removal())),
            )
            .await?
        } else {
            // Legacy Remove permits this foreign stream; preserve acceptance, deny trusted provenance.
            let mut source = next_event(
                &h.p_obj,
                &h.a,
                VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.removal())),
            )
            .await?;
            source.0.key = KvKey::from(DeviceLogDescriptor::from(h.b.user_id()));
            let signed = sign_event(
                source.clone().to_generic(),
                h.a.device_id().clone(),
                &h.a.device_creds.key_manager()?.dsa,
            )?;
            send(&h.server, signed).await?;
            source
        };
        let log = PersistentVault::from(h.p_obj.clone())
            .get_vault_log_artifact(h.a.vault_name.clone())
            .await?;
        let wire = serde_json::to_value(log.0.value)?;
        assert!(
            wire.get("producerSourceRefs")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty),
            "no backfilled/foreign-stream verified proof"
        );
        let input = ProducerReference {
            version: 1,
            kind: ProducerKind::RemoveDevice,
            request_id: None,
            request_stage: ProducerSourceRef {
                event_id: source.obj_id(),
                body_commitment: proof::commitment(&source)?,
            },
            decision_stage: None,
        };
        let response = proof::observe(&h.server, h.a.user(), input).await;
        if let Ok(response) = response {
            let observed = response
                .producer_observation
                .context("RED: unsupported provenance must be explicit Unknown")?;
            assert_eq!(observed.canonical_state, ProducerCanonicalState::Unknown);
            assert!(
                observed.request_id.is_none(),
                "no target/body/latest fallback to a fabricated source origin"
            );
        } else {
            assert!(
                !historical,
                "a well-formed own-stream historical source is unknown, not an invalid reference"
            );
        }
    }
    Ok(())
}
#[tokio::test]
async fn correlation_remote_three_device_mode_never_rebinds_to_automatic_after_context_change()
-> Result<()> {
    let h = Seed::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    let server = h.server().await?;
    let (source, signed, id) = h.deletion().await?;
    send(&server, signed.clone()).await?;
    let original = proof::source_ref(&h.log().await?, &source, &id)?;
    assert_eq!(original["completionMode"], "remotePending");
    let pv = PersistentVault::from(h.p.clone());
    let mut context = pv.get_vault(h.actor.vault_name.clone()).await?;
    let target = context
        .0
        .value
        .members()
        .into_iter()
        .find(|u| u.user_data.device.device_id != *h.actor.device_id())
        .context("peer context fixture")?
        .user_data;
    context.0.key.obj_id = context.obj_id().next();
    context.0.value =
        context
            .0
            .value
            .update_membership(UserMembership::Outsider(UserDataOutsider {
                user_data: target,
                status: UserDataOutsiderStatus::Declined,
            }));
    h.repo.save(context).await?; // Negative newer-context fixture, not an asserted ordinary removal.
    let _ = proof::observe(&server, h.actor.user(), proof::reference(&source, &id)?).await;
    let log = h.log().await?;
    let wire = serde_json::to_value(&log.0.value)?;
    assert!(
        wire.get("automaticDeleteApplication")
            .is_none_or(Value::is_null)
    );
    let current_ref = proof::source_ref(&log, &source, &id)?;
    assert!(
        current_ref["completionMode"] != "automaticPending"
            && current_ref["completionMode"] != "automaticComplete"
    );
    assert!(
        !pv.get_vault(h.actor.vault_name.clone())
            .await?
            .0
            .value
            .deletion_results
            .get(&id)
            .is_some_and(|r| r.accepted),
        "old remote3 consent never derives from today's count"
    );
    assert!(send(&server, signed).await.is_err());
    Ok(())
}
