use super::*;
use meta_secret_core::node::api::{SignedActionPayload, SsRequest};
use meta_secret_core::node::common::model::secret::{
    SsDeclineData, SsDistributionId, SsRecoveryId,
};
use meta_secret_core::node::db::descriptors::{
    local_removal_envelope::LocalRemovalEnvelopeDescriptor,
    shared_secret_descriptor::{SsLogDescriptor, SsWorkflowDescriptor},
};
use meta_secret_core::node::db::events::{
    local_removal_envelope::{LocalRemovalEnvelopeObject, LocalRemovalEnvelopeRegistry},
    shared_secret_event::SsWorkflowObject,
};
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;

#[tokio::test]
async fn liveboard_guarded_wrong_stream_nonce_key_target_and_initiator_have_no_effect() -> Result<()>
{
    let h = Harness::new().await?;
    h.request().await?;
    let id = h.pending_id().await?;
    let event = next_event(
        &h.p_obj,
        &h.c,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(h.decision(false))),
    )
    .await?;
    let valid = guarded(&h.c, &event, &id, 1)?;
    let key = h.c.device_creds.key_manager()?.dsa;
    let stream = SignedAction::sign(
        valid.payload.clone(),
        h.c.device_id().clone(),
        "different-stream".into(),
        valid.nonce,
        &key,
    )?;
    let nonce = SignedAction::sign(
        valid.payload.clone(),
        h.c.device_id().clone(),
        valid.stream.clone(),
        valid.nonce + 1,
        &key,
    )?;
    let wrong_key = SignedAction::sign(
        valid.payload.clone(),
        h.c.device_id().clone(),
        valid.stream.clone(),
        valid.nonce,
        &h.a.device_creds.key_manager()?.dsa,
    )?;
    let mut attempts = vec![stream, nonce, wrong_key];
    for actor in [&h.a, &h.b] {
        let mut update = h.decision(false);
        update.sender = member(actor);
        let event = next_event(
            &h.p_obj,
            actor,
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)),
        )
        .await?;
        attempts.push(guarded(actor, &event, &id, 1)?);
    }
    for action in attempts {
        let before = h.p_obj.repo.get_db().await;
        assert!(
            send(&h.server, action).await.is_err(),
            "valid metadata/signature alone does not authorize this actor/stream/nonce"
        );
        assert!(
            h.p_obj.repo.get_db().await == before,
            "rejected guarded action cannot alter canonical state or source tail"
        );
    }
    send(&h.server, valid).await?;
    assert_eq!(
        result_record(&h.log().await?, &id)?["responderId"],
        serde_json::to_value(h.c.device_id())?
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_signed_canonical_log_and_local_registry_uploads_are_not_client_writes_or_replication()
-> Result<()> {
    let h = Harness::new().await?;
    h.request().await?;
    let log = PersistentVault::from(h.p_obj.clone())
        .get_vault_log_artifact(h.a.vault_name.clone())
        .await?;
    let registry = LocalRemovalEnvelopeObject(KvLogEvent {
        key: KvKey::from(LocalRemovalEnvelopeDescriptor(h.c.user_id())),
        value: LocalRemovalEnvelopeRegistry {
            producer_context: None,
            owner: h.c.user_id(),
            first_managed_removal_nonce: 1,
            entries: vec![],
        },
    });
    for event in [log.to_generic(), registry.clone().to_generic()] {
        let action = sign_event(
            event,
            h.c.device_id().clone(),
            &h.c.device_creds.key_manager()?.dsa,
        )?;
        let before = h.p_obj.repo.get_db().await;
        assert!(
            send(&h.server, action).await.is_err(),
            "server-owned/local-only records are never authorized by a client signature"
        );
        assert!(h.p_obj.repo.get_db().await == before);
    }
    h.p_obj.repo.save(registry).await?;
    let response = h
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
            SsRequest {
                sender: h.a.user(),
                ss_log: ArtifactId::from(SsLogDescriptor::from(h.a.vault_name.clone())),
            },
        ))))
        .await?;
    if let DataSyncResponse::Data(events) = response {
        assert!(events.0.iter().all(|event| !matches!(event, meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent::LocalRemovalEnvelope(_))));
    }
    Ok(())
}

#[tokio::test]
async fn liveboard_recovery_sender_cannot_decline_its_own_remote_request() -> Result<()> {
    let h = Harness::new().await?;
    let claim = super::recovery::pending_claim(&h).await?;
    let id = SsRecoveryId {
        claim_id: claim.dist_claim_id.clone(),
        sender: h.a.device_id().clone(),
        distribution_id: SsDistributionId {
            pass_id: claim.dist_claim_id.pass_id.clone(),
            receiver: h.a.device_id().clone(),
        },
    };
    let event = SsWorkflowObject::Decline(KvLogEvent {
        key: KvKey::from(SsWorkflowDescriptor::Decline(id)),
        value: SsDeclineData {
            vault_name: h.a.vault_name.clone(),
            claim_id: claim.id.clone(),
            receiver_id: h.a.device_id().clone(),
        },
    });
    let before = h.p_obj.repo.get_db().await;
    let signed = sign_event(
        event.to_generic(),
        h.a.device_id().clone(),
        &h.a.device_creds.key_manager()?.dsa,
    )?;
    assert!(matches!(signed.payload, SignedActionPayload::Event(_)));
    let result = send(&h.server, signed).await;
    let state = PersistentSharedSecret::from(h.p_obj.clone())
        .get_ss_log_obj(h.a.vault_name.clone())
        .await?;
    assert!(
        state.claims[&claim.id].status == claim.status,
        "SECURITY RED: a validly signed non-receiver response changed remote recovery status"
    );
    assert!(
        result.is_err(),
        "a current member who is not this claim's receiver is not an authorized responder"
    );
    assert!(h.p_obj.repo.get_db().await == before);
    Ok(())
}
