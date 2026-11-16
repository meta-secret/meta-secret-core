use super::{recovery_inflight_fixture::Inflight, recovery_intake_fixture::assert_clean, *};
use meta_secret_core::node::db::repo::generic_db::FindOneQuery;
use meta_secret_core::node::{
    api::SsRecoveryCompletion,
    app::orchestrator::MetaOrchestrator,
    common::model::secret::SsDistributionStatus,
    db::{
        descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
        objects::persistent_shared_secret::PersistentSharedSecret,
    },
    security::sign_recovery_completion,
};

async fn packet(h: &Inflight<InMemKvLogEventRepo>, accepted: bool) -> Result<SignedAction> {
    h.pull_canonical().await?;
    let authoritative = h.log().await?;
    if h.local
        .repo
        .find_one(authoritative.obj_id())
        .await?
        .is_some()
    {
        h.local.repo.replace(authoritative).await?;
    } else {
        h.local.repo.save(authoritative).await?;
    }
    let owner = MetaOrchestrator {
        p_obj: h.local.clone(),
        user_creds: h.base.b.clone(),
    };
    if accepted {
        owner.accept_recover(h.source.0.value.id.clone()).await?;
    } else {
        owner.decline_recover(h.source.0.value.id.clone()).await?;
    }
    let id = h
        .source
        .0
        .value
        .recovery_db_ids()
        .into_iter()
        .find(|id| &id.distribution_id.receiver == h.base.b.device_id())
        .context("actual original addressed B")?;
    let descriptor = if accepted {
        SsWorkflowDescriptor::Recovery(id)
    } else {
        SsWorkflowDescriptor::Decline(id)
    };
    let body = h
        .local
        .find_tail_event(descriptor)
        .await?
        .context("actual Core B response saved locally")?;
    let signed = sign_event(
        body.to_generic(),
        h.base.b.device_id().clone(),
        &h.base.b.device_creds.key_manager()?.dsa,
    )?;
    signed.verify(&h.base.b.device().keys.dsa_pk)?;
    Ok(signed)
}
async fn old_pending(h: &Inflight<InMemKvLogEventRepo>) -> Result<()> {
    let mut snapshot = h.source.clone();
    snapshot.0.key = snapshot.0.key.next(); // A distinct permitted snapshot nonce, not replay of original R.
    send(
        &h.server,
        sign_event(
            snapshot.to_generic(),
            h.base.a.device_id().clone(),
            &h.base.a.device_creds.key_manager()?.dsa,
        )?,
    )
    .await
}
async fn outcome(accepted: bool) -> Result<()> {
    let h = Inflight::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    send(&h.server, h.signed.clone()).await?;
    send(&h.server, packet(&h, accepted).await?).await?;
    let first = h.log().await?;
    let winner = first
        .0
        .value
        .liveboard_recovery_results
        .get(&h.source.0.value.id)
        .context("actual first B result")?
        .clone();
    assert_eq!(winner.responder_id, *h.base.b.device_id());
    assert_eq!(
        serde_json::to_value(&winner)?["outcome"],
        if accepted { "accepted" } else { "declined" }
    );
    send(&h.server, packet(&h, !accepted).await?).await?;
    assert!(h.log().await?.0.value.liveboard_recovery_results[&h.source.0.value.id] == winner);
    old_pending(&h).await?;
    let after = h.log().await?;
    assert_eq!(
        after.0.value.claims[&h.source.0.value.id]
            .status
            .get(h.base.b.device_id()),
        Some(&if accepted {
            SsDistributionStatus::Sent
        } else {
            SsDistributionStatus::Declined
        })
    );
    assert!(after.0.value.liveboard_recovery_results[&h.source.0.value.id] == winner);
    assert!(after.0.value.pending_recovery_request_intake.is_none());
    if accepted {
        let id = h
            .source
            .0
            .value
            .recovery_db_ids()
            .into_iter()
            .find(|id| &id.distribution_id.receiver == h.base.b.device_id())
            .context("approved B")?;
        let completion = sign_recovery_completion(
            SsRecoveryCompletion {
                vault_name: h.base.a.vault_name.clone(),
                recovery_id: id,
                receiver_status: SsDistributionStatus::Sent,
            },
            h.base.a.device_id().clone(),
            &h.base.a.device_creds.key_manager()?.dsa,
        )?;
        assert_clean(
            &h.server
                .handle_client_request(SyncRequest::Read(Box::new(
                    ReadSyncRequest::SsRecoveryCompletion(completion),
                )))
                .await?,
        )?;
        let mut snapshot = h.source.clone();
        snapshot.0.key.obj_id = snapshot.obj_id().next().next();
        send(
            &h.server,
            sign_event(
                snapshot.to_generic(),
                h.base.a.device_id().clone(),
                &h.base.a.device_creds.key_manager()?.dsa,
            )?,
        )
        .await?;
        let done = h.log().await?;
        assert_eq!(
            done.0.value.claims[&h.source.0.value.id]
                .status
                .get(h.base.a.device_id()),
            Some(&SsDistributionStatus::Delivered)
        );
        assert!(done.0.value.liveboard_recovery_results[&h.source.0.value.id] == winner);
        assert!(done.0.value.pending_recovery_request_intake.is_none());
    }
    assert!(send(&h.server, h.signed.clone()).await.is_err());
    Ok(())
}
#[tokio::test]
async fn recovery_inflight_real_remaining_b_approve_winner_completion_and_delayed_pending_are_monotonic()
-> Result<()> {
    outcome(true).await
}
#[tokio::test]
async fn recovery_inflight_real_remaining_b_decline_and_late_opposite_pending_do_not_reset_winner()
-> Result<()> {
    outcome(false).await
}

#[tokio::test]
async fn recovery_inflight_requester_and_removed_original_receiver_have_no_response_right()
-> Result<()> {
    use meta_secret_core::node::{
        common::model::secret::SsDeclineData, db::events::shared_secret_event::SsWorkflowObject,
    };
    let h = Inflight::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    send(&h.server, h.signed.clone()).await?;
    let valid = packet(&h, false).await?;
    let from_requester = sign_event(
        valid.event()?.clone(),
        h.base.a.device_id().clone(),
        &h.base.a.device_creds.key_manager()?.dsa,
    )?;
    let id = h
        .source
        .0
        .value
        .recovery_db_ids()
        .into_iter()
        .find(|id| &id.distribution_id.receiver == h.base.c.device_id())
        .context("original removed C still addressed")?;
    let body = SsWorkflowObject::Decline(KvLogEvent {
        key: KvKey::from(SsWorkflowDescriptor::Decline(id)),
        value: SsDeclineData {
            vault_name: h.base.a.vault_name.clone(),
            claim_id: h.source.0.value.id.clone(),
            receiver_id: h.base.c.device_id().clone(),
        },
    });
    let from_removed = sign_event(
        body.to_generic(),
        h.base.c.device_id().clone(),
        &h.base.c.device_creds.key_manager()?.dsa,
    )?;
    for action in [from_requester, from_removed] {
        let before = h.repo.get_db().await;
        assert!(send(&h.server, action).await.is_err());
        assert!(h.repo.get_db().await == before);
    }
    Ok(())
}
#[tokio::test]
async fn recovery_inflight_wrong_actor_vault_stream_nonce_signature_and_canonical_upload_stay_rejected()
-> Result<()> {
    let h = Inflight::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let actor = sign_event(
        h.source.clone().to_generic(),
        h.base.b.device_id().clone(),
        &h.base.b.device_creds.key_manager()?.dsa,
    )?;
    let mut source = h.source.clone();
    source.0.value.vault_name = "wrong-vault".into();
    let vault = sign_event(
        source.to_generic(),
        h.base.a.device_id().clone(),
        &h.base.a.device_creds.key_manager()?.dsa,
    )?;
    let mut stream = h.signed.clone();
    stream.stream = "wrong-stream".into();
    let mut nonce = h.signed.clone();
    nonce.nonce += 1;
    let mut signature = h.signed.clone();
    signature.signature = actor.signature.clone();
    let canonical = sign_event(
        h.ss_base.clone().to_generic(),
        h.base.a.device_id().clone(),
        &h.base.a.device_creds.key_manager()?.dsa,
    )?;
    for action in [actor, vault, stream, nonce, signature, canonical] {
        let before = h.repo.get_db().await;
        assert!(send(&h.server, action).await.is_err());
        assert!(h.repo.get_db().await == before);
    }
    assert!(
        !h.log()
            .await?
            .0
            .value
            .claims
            .contains_key(&h.source.0.value.id)
    );
    assert_eq!(
        PersistentSharedSecret::from(h.local.clone())
            .get_ss_log_obj(h.base.a.vault_name.clone())
            .await?
            .claims
            .values()
            .filter(|c| c.id == h.source.0.value.id)
            .count(),
        0
    );
    Ok(())
}
