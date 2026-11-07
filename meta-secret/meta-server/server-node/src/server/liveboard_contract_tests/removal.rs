use super::*;
use meta_secret_core::node::db::repo::generic_db::FindOneQuery;

#[tokio::test]
async fn liveboard_historical_pending_gets_one_durable_reference_without_rewriting_request()
-> Result<()> {
    let h = Harness::new().await?;
    h.historical_pending().await?;
    let signed_shape = canonical_json(&h.removal())?;
    h.refresh().await?;
    let id = h.pending_id().await?;
    let log = h.log().await?;
    let binding = &log["liveboardRemovalPending"][0];
    assert!(
        canonical_json(&binding["request"])? == signed_shape,
        "historical signed request bytes must not change"
    );
    assert!(
        h.p_obj
            .get_object_events_from_beginning(DeviceLogDescriptor::from(h.a.user_id()))
            .await?
            .is_empty(),
        "initialization must not infer identity by inventing a DeviceLog origin"
    );
    refresh(&h.reopen().await?, &h.a).await?;
    assert_eq!(
        h.pending_id().await?,
        id,
        "reopen must preserve continuous pending identity"
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_duplicate_source_nonce_keeps_pending_reference_and_retry_rotates_it()
-> Result<()> {
    let h = Harness::new().await?;
    h.request().await?;
    let first = h.pending_id().await?;
    h.request().await?;
    assert_eq!(
        h.pending_id().await?,
        first,
        "same body at a fresh source nonce is still the same pending occurrence"
    );
    h.decide(false).await?;
    let terminal = h.log().await?;
    assert_eq!(result_record(&terminal, &first)?["outcome"], "declined");
    h.request().await?;
    let retry = h.pending_id().await?;
    assert_ne!(
        retry, first,
        "a terminal decision followed by an identical request is a new occurrence"
    );
    let before = h.p_obj.repo.get_db().await;
    let event = next_event(
        &h.p_obj,
        &h.c,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(h.decision(true))),
    )
    .await?;
    assert!(
        send(&h.server, guarded(&h.c, &event, &first, 1)?)
            .await
            .is_err(),
        "stale authenticated decision must not bind to retry"
    );
    assert!(
        h.p_obj.repo.get_db().await == before,
        "rejected stale ref must have no persisted effects"
    );
    assert_eq!(h.pending_id().await?, retry);
    Ok(())
}

#[tokio::test]
async fn liveboard_legacy_first_response_retains_verified_terminal_result_after_restart()
-> Result<()> {
    let h = Harness::new().await?;
    h.request().await?;
    let id = h.pending_id().await?;
    h.decide(false).await?;
    let first = h.log().await?;
    let record = result_record(&first, &id)?.clone();
    assert_eq!(
        record["requesterId"],
        serde_json::to_value(h.a.device_id())?
    );
    assert_eq!(record["targetId"], serde_json::to_value(h.b.device_id())?);
    assert_eq!(
        record["responderId"],
        serde_json::to_value(h.c.device_id())?
    );
    assert_eq!(record["outcome"], "declined");
    assert_eq!(record["applicationState"], "applied");
    h.decide(true).await?;
    refresh(&h.reopen().await?, &h.a).await?;
    assert_eq!(
        result_record(&h.log().await?, &id)?,
        &record,
        "late opposite legacy response cannot replace winner"
    );
    let vault = PersistentVault::from(h.p_obj.clone())
        .get_vault(h.a.vault_name.clone())
        .await?;
    assert_eq!(vault.0.value.members().len(), 3);
    assert!(
        !serde_json::to_string(&record)?.contains("shareTransfers"),
        "compact result must contain no transfer material"
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_guarded_removal_rejects_wrong_actor_reference_body_and_replay_without_mutation()
-> Result<()> {
    let h = Harness::new().await?;
    h.request().await?;
    let id = h.pending_id().await?;
    let event = next_event(
        &h.p_obj,
        &h.c,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(h.decision(false))),
    )
    .await?;
    let mut changed_body = event.clone();
    if let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) =
        &mut changed_body.0.value
    {
        update.request.target = member(&h.c);
        update.target = member(&h.c);
    }
    let attempts = [
        guarded(&h.a, &event, &id, 1)?,
        guarded(&h.c, &event, "00000000-0000-4000-8000-000000000000", 1)?,
        guarded(&h.c, &changed_body, &id, 1)?,
        guarded(&h.c, &event, &id, 2)?,
    ];
    for attempt in attempts {
        let before = h.p_obj.repo.get_db().await;
        assert!(
            send(&h.server, attempt).await.is_err(),
            "invalid signed guarded decision must be rejected"
        );
        assert!(
            h.p_obj.repo.get_db().await == before,
            "invalid guarded decision must have no database effects"
        );
    }
    let valid = guarded(&h.c, &event, &id, 1)?;
    send(&h.server, valid.clone()).await?;
    let before = h.p_obj.repo.get_db().await;
    assert!(
        send(&h.server, valid).await.is_err(),
        "consumed signed nonce cannot be replayed"
    );
    assert!(h.p_obj.repo.get_db().await == before);
    assert_eq!(
        result_record(&h.log().await?, &id)?["responderId"],
        serde_json::to_value(h.c.device_id())?
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_capability_probe_reports_versions_without_canonical_writes() -> Result<()> {
    let h = Harness::new().await?;
    let before = h.p_obj.repo.get_db().await;
    let request: SyncRequest =
        serde_json::from_value(json!({"read":{"liveboardCapabilities":{"version":1}}}))
            .context("RED: capability probe is not an explicit versioned read request")?;
    let response = serde_json::to_value(h.server.handle_client_request(request).await?)?;
    assert_eq!(response["liveboardCapabilities"]["projectionVersion"], 1);
    assert_eq!(
        response["liveboardCapabilities"]["guardedRemovalVersion"],
        1
    );
    assert!(
        h.p_obj.repo.get_db().await == before,
        "capability probing must not write canonical state"
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_corrupt_pending_binding_is_unavailable_instead_of_reallocated() -> Result<()> {
    let h = Harness::new().await?;
    h.historical_pending().await?;
    h.refresh().await?;
    let id = h.pending_id().await?;
    let p_vault = PersistentVault::from(h.p_obj.clone());
    let log = p_vault
        .get_vault_log_artifact(h.a.vault_name.clone())
        .await?;
    let mut wire = serde_json::to_value(&log)?;
    let value = wire
        .get_mut("value")
        .context("VaultLog tuple wire representation")?;
    value["liveboardRemovalPending"][0]["requestId"] = json!("not-a-uuid");
    let corrupt: VaultLogObject = serde_json::from_value(wire)?;
    h.p_obj.repo.replace(corrupt).await?;
    let before = h.p_obj.repo.get_db().await;
    assert!(
        h.refresh().await.is_err(),
        "corrupt occurrence identity must fail closed"
    );
    assert!(
        h.p_obj.repo.get_db().await == before,
        "refresh cannot repair corruption by rotating identity"
    );
    assert!(h.p_obj.repo.find_one(log.obj_id()).await?.is_some());
    assert_ne!(id, "not-a-uuid");
    Ok(())
}
