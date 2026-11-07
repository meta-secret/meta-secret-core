//! Positive wire/lifecycle tests expose the missing variant as semantic RED.
use super::fixture::*;
use anyhow::Result;
use meta_secret_core::node::{
    api::*,
    common::model::IdString,
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::{
            generic_log_event::*,
            object_id::{ArtifactId, Next},
            vault::vault_log_event::*,
        },
        repo::generic_db::*,
    },
};
#[tokio::test]
async fn current_members_may_use_own_next_empty_slot_without_prior_command() -> Result<()> {
    let h = H::new().await?;
    for actor in [&h.a, &h.b, &h.c] {
        let before = h.p.repo.inner.get_db().await;
        let auth = h.authorization(actor).await?;
        let response = h.submit(auth.clone()).await?;
        stable(&response, &auth)?;
        let id: ArtifactId = serde_json::from_value(auth["eventId"].clone())?;
        let after = h.p.repo.inner.get_db().await;
        assert_eq!(after.len(), before.len() + 1);
        for (k, v) in before {
            assert!(
                after.get(&k) == Some(&v),
                "no canonical/member/claim/business mutation from no-op"
            );
        }
        assert!(after.contains_key(&id));
        assert_eq!(h.typed_tail(actor).await?, Some(id.clone()));
        assert_eq!(h.next_tail(actor).await?, Some(id.next()));
    }
    Ok(())
}
#[tokio::test]
async fn exact_replay_after_real_next_command_keeps_stable_receipt_and_advances_contiguous_tail()
-> Result<()> {
    let h = H::new().await?;
    let auth = h.authorization(&h.a).await?;
    let first = stable(&h.submit(auth.clone()).await?, &auth)?;
    let id: ArtifactId = serde_json::from_value(auth["eventId"].clone())?;
    let event = h
        .business(
            &h.a,
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.removal())),
        )
        .await?;
    assert_eq!(event.obj_id(), id.clone().next());
    assert!(h.p.repo.find_one(event.obj_id()).await? == Some(event.clone().to_generic()));
    let before = h.p.repo.inner.get_db().await;
    let replay = stable(&h.submit(auth.clone()).await?, &auth)?;
    assert_eq!(first, replay);
    assert!(before == h.p.repo.inner.get_db().await);
    assert_eq!(h.next_tail(&h.a).await?, Some(event.obj_id().next()));
    Ok(())
}
#[tokio::test]
async fn changed_signed_commitment_at_occupied_noop_conflicts_after_n_plus_one() -> Result<()> {
    let h = H::new().await?;
    let auth = h.authorization(&h.a).await?;
    h.submit(auth.clone()).await?;
    h.business(
        &h.a,
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.removal())),
    )
    .await?;
    let before = h.p.repo.inner.get_db().await;
    let mut other = auth;
    other["clientEntryCommitment"] = serde_json::Value::String("f".repeat(64));
    let other = sign(&h.a, other)?;
    assert!(h.submit(other).await.is_err());
    assert!(before == h.p.repo.inner.get_db().await);
    Ok(())
}
#[tokio::test]
async fn ordinary_event_already_at_n_prevents_noop_replacement() -> Result<()> {
    let h = H::new().await?;
    let auth = h.authorization(&h.a).await?;
    decode(auth.clone())?;
    h.business(
        &h.a,
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.removal())),
    )
    .await?;
    let before = h.p.repo.inner.get_db().await;
    assert!(h.submit(auth).await.is_err());
    assert!(before == h.p.repo.inner.get_db().await);
    Ok(())
}
#[tokio::test]
async fn noop_first_prevents_late_business_at_same_slot_without_new_pending_action() -> Result<()> {
    let h = H::new().await?;
    let auth = h.authorization(&h.a).await?;
    let id: ArtifactId = serde_json::from_value(auth["eventId"].clone())?;
    h.submit(auth).await?;
    let mut key = meta_secret_core::node::db::events::kv_log_event::KvKey::from(
        DeviceLogDescriptor::from(h.a.user_id()),
    );
    key.obj_id = id;
    let body = meta_secret_core::node::db::events::vault::device_log_event::DeviceLogObject(
        meta_secret_core::node::db::events::kv_log_event::KvLogEvent {
            key,
            value: VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.removal())),
        },
    );
    let action = meta_secret_core::node::security::sign_event(
        body.to_generic(),
        h.a.device_id().clone(),
        &h.a.device_creds.key_manager()?.dsa,
    )?;
    let before = h.p.repo.inner.get_db().await;
    assert!(
        h.server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                action
            ))))
            .await
            .is_err()
    );
    assert!(before == h.p.repo.inner.get_db().await);
    Ok(())
}
#[tokio::test]
async fn capability_old_shape_defaults_noop_zero_and_new_server_advertises_one() -> Result<()> {
    let old: LiveboardCapabilities = serde_json::from_value(
        serde_json::json!({"projectionVersion":1,"guardedRemovalVersion":1}),
    )?;
    let old_wire = serde_json::to_value(old)?;
    assert_eq!(
        old_wire
            .get("deviceTransportNoopVersion")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        0
    );
    let h = H::new().await?;
    let response = h
        .server
        .handle_client_request(SyncRequest::Read(Box::new(
            ReadSyncRequest::LiveboardCapabilities(LiveboardCapabilitiesRequest { version: 1 }),
        )))
        .await?;
    assert_eq!(
        serde_json::to_value(response)?["liveboardCapabilities"]["deviceTransportNoopVersion"],
        1,
        "RED: dedicated permission requires explicit capability"
    );
    Ok(())
}

#[tokio::test]
async fn existing_ordinary_signed_command_wire_and_actual_dispatch_compatibility_control()
-> Result<()> {
    let h = H::new().await?;
    let source = h
        .business(
            &h.a,
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.removal())),
        )
        .await?;
    let action = meta_secret_core::node::security::sign_event(
        source.clone().to_generic(),
        h.a.device_id().clone(),
        &h.a.device_creds.key_manager()?.dsa,
    )?;
    action.verify(&h.a.device().keys.dsa_pk)?;
    assert_eq!(action.stream, source.obj_id().fqdn.id_str());
    assert_eq!(action.nonce, source.obj_id().id.curr as u64);
    let bytes = serde_json::to_vec(&SyncRequest::Write(Box::new(WriteSyncRequest::Event(
        action,
    ))))?;
    let decoded: SyncRequest = serde_json::from_slice(&bytes)?;
    assert_eq!(serde_json::to_vec(&decoded)?, bytes);
    assert!(h.p.repo.find_one(source.obj_id()).await? == Some(source.clone().to_generic()));
    assert_eq!(h.next_tail(&h.a).await?, Some(source.obj_id().next()));
    Ok(())
}
#[tokio::test]
async fn approved_flat_signature_fixture_uses_actual_dsa_and_canonical_purpose_bytes_control()
-> Result<()> {
    let h = H::new().await?;
    let mut authorization = h.authorization(&h.a).await?;
    let signature: meta_secret_core::crypto::encoding::base64::Base64Text = serde_json::from_value(
        authorization
            .as_object_mut()
            .unwrap()
            .remove("signature")
            .unwrap(),
    )?;
    let bytes = String::from_utf8(meta_secret_core::node::security::canonical_json(
        &authorization,
    )?)?;
    h.a.device().keys.dsa_pk.verify(&bytes, &signature)?;
    authorization["purpose"] = serde_json::json!("meta-secret.event.v1");
    let changed = String::from_utf8(meta_secret_core::node::security::canonical_json(
        &authorization,
    )?)?;
    assert!(
        h.a.device()
            .keys
            .dsa_pk
            .verify(&changed, &signature)
            .is_err()
    );
    // Fixture crypto control only: new server no-op verifier remains behind valid wire gate.
    Ok(())
}
