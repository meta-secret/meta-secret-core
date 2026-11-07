use super::{business::vault_request, repository::Repo, support::*, *};
use meta_secret_core::node::{
    api::SsRequest,
    db::{
        descriptors::shared_secret_descriptor::SsLogDescriptor,
        events::device_transport_noop::DeviceTransportNoopObject,
        objects::persistent_shared_secret::PersistentSharedSecret,
    },
};
use std::sync::atomic::Ordering;

async fn visible(direct: bool) -> Result<()> {
    let repo = Arc::new(Repo::new(Arc::new(InMemKvLogEventRepo::default())));
    let h = Seed::new(repo.clone(), 2).await?;
    let server = h.server().await?;
    project_initial_split_records(&h).await?;
    let auth = signed_value(&h.actor, next(&h).await?)?;
    submit(&server, &auth).await?;
    let signature = auth["signature"]
        .as_str()
        .context("actual technical signature")?;
    let request = vault_request(&h);
    let gateway = crate::server::server_data_sync::ServerSyncGateway::from(h.p.clone());
    let value = if direct {
        serde_json::to_value(gateway.vault_replication(request).await?)?
    } else {
        serde_json::to_value(
            server
                .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(request))))
                .await?,
        )?
    };
    assert!(!value.to_string().contains(signature));
    assert!(!value.to_string().contains("deviceTransportNoop"));
    let request = SsRequest {
        sender: h.actor.user(),
        ss_log: ArtifactId::from(SsLogDescriptor::from(h.actor.vault_name.clone())),
    };
    let value = if direct {
        serde_json::to_value(
            gateway
                .ss_replication(request, server.init().await?.device.device_id)
                .await?,
        )?
    } else {
        serde_json::to_value(
            server
                .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                    request,
                ))))
                .await?,
        )?
    };
    assert!(!value.to_string().contains(signature));
    assert!(!value.to_string().contains("deviceTransportNoop"));
    Ok(())
}
#[tokio::test]
async fn noop_b_real_outer_canonical_and_ss_reads_omit_marker_signature() -> Result<()> {
    visible(false).await
}
#[tokio::test]
async fn noop_b_real_direct_canonical_and_ss_reads_omit_marker_signature() -> Result<()> {
    visible(true).await
}

#[tokio::test]
async fn noop_b_marker_tail_rejects_outer_direct_before_any_lookup() -> Result<()> {
    let repo = Arc::new(Repo::new(Arc::new(InMemKvLogEventRepo::default())));
    let h = Seed::new(repo.clone(), 2).await?;
    let server = h.server().await?;
    let id = next(&h).await?;
    submit(&server, &signed_value(&h.actor, id.clone())?).await?;
    let before = repo.inner.get_db().await;
    let writes = repo.writes.load(Ordering::SeqCst);
    let mut request = vault_request(&h);
    request.tail.vault_log = id.clone();
    let gateway = crate::server::server_data_sync::ServerSyncGateway::from(h.p.clone());
    repo.reads.lock().unwrap().clear();
    let error = server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            request.clone(),
        ))))
        .await
        .err()
        .context("forbidden marker tail accepted")?;
    assert_eq!(error.to_string(), "InvalidPublicReadTail");
    let error = gateway
        .vault_replication(request)
        .await
        .err()
        .context("direct marker tail accepted")?;
    assert_eq!(error.to_string(), "InvalidPublicReadTail");
    assert!(!repo.reads.lock().unwrap().contains(&id));
    assert!(repo.inner.get_db().await == before);
    assert_eq!(repo.writes.load(Ordering::SeqCst), writes);
    Ok(())
}
#[tokio::test]
async fn noop_b_generic_signed_event_cannot_mint_transport_marker() -> Result<()> {
    let h = Seed::new(Arc::new(InMemKvLogEventRepo::default()), 2).await?;
    let server = h.server().await?;
    let authorization: DeviceTransportNoop =
        serde_json::from_value(signed_value(&h.actor, next(&h).await?)?)?;
    let marker = DeviceTransportNoopObject::new(
        authorization,
        &h.actor.user_id(),
        &h.actor.device().keys.dsa_pk,
    )?;
    let before = h.repo.get_db().await;
    let packet = sign_event(
        marker.to_generic(),
        h.actor.device_id().clone(),
        &h.actor.device_creds.key_manager()?.dsa,
    )?;
    assert!(send(&server, packet).await.is_err());
    assert!(h.repo.get_db().await == before);
    Ok(())
}

async fn corrupt_canonical(direct: bool) -> Result<()> {
    // Labelled invalid DB records, not a membership/authorization lifecycle.
    for kind in 0..3 {
        let h = Seed::new(Arc::new(InMemKvLogEventRepo::default()), 2).await?;
        let server = h.server().await?;
        project_initial_split_records(&h).await?;
        let request = vault_request(&h);
        refresh(&server, &h.actor).await?; // Same correctly addressed public route is healthy first.
        let (key, event) = match kind {
            0 => (
                request.tail.vault.clone(),
                h.repo
                    .find_one(h.workflows[0].clone())
                    .await?
                    .context("actual encrypted workflow")?,
            ),
            1 => {
                let key = request.tail.vault_log.clone();
                let mut log = h
                    .repo
                    .find_one(key.clone())
                    .await?
                    .context("actual canonical log")?
                    .vault_log()?;
                log.0.key.obj_id = key.clone().next();
                (key, log.to_generic())
            }
            _ => {
                let mut log = PersistentSharedSecret::from(h.p.clone())
                    .find_ss_log_tail_event(h.actor.vault_name.clone())
                    .await?
                    .context("actual SsLog")?;
                for claim in log.0.value.claims.values_mut() {
                    claim.vault_name = "foreign-clean-context".into();
                }
                (log.obj_id(), log.to_generic())
            }
        };
        h.repo.db.lock().await.insert(key.clone(), event);
        let before = h.repo.get_db().await;
        let gateway = crate::server::server_data_sync::ServerSyncGateway::from(h.p.clone());
        let result = if kind == 2 {
            let request = SsRequest {
                sender: h.actor.user(),
                ss_log: key,
            };
            if direct {
                gateway
                    .ss_replication(request, server.init().await?.device.device_id)
                    .await
                    .map(|v| serde_json::to_value(v).unwrap())
            } else {
                server
                    .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                        request,
                    ))))
                    .await
                    .map(|v| serde_json::to_value(v).unwrap())
            }
        } else if direct {
            gateway
                .vault_replication(request)
                .await
                .map(|v| serde_json::to_value(v).unwrap())
        } else {
            server
                .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(request))))
                .await
                .map(|v| serde_json::to_value(v).unwrap())
        };
        assert!(
            result.is_err(),
            "allowed-key corrupt kind/key/Vault must fail closed before public payload"
        );
        assert!(
            h.repo.get_db().await == before,
            "allowed-key corruption triggered canonical mutation"
        );
    }
    Ok(())
}
#[tokio::test]
async fn noop_b_outer_allowed_canonical_key_requires_actual_kind_key_vault() -> Result<()> {
    corrupt_canonical(false).await
}
#[tokio::test]
async fn noop_b_direct_allowed_canonical_key_requires_actual_kind_key_vault() -> Result<()> {
    corrupt_canonical(true).await
}

#[tokio::test]
async fn noop_b_actual_queued_readback_error_never_exports_private_backend_detail() -> Result<()> {
    let repo = Arc::new(Repo::new(Arc::new(InMemKvLogEventRepo::default())));
    let h = Seed::new(repo.clone(), 2).await?;
    let server = h.server().await?;
    let auth = signed_value(&h.actor, next(&h).await?)?;
    repo.arm(4);
    let transfer = server.get_data_transfer();
    let response = tokio::select! {
        run=server.run()=>{run?;anyhow::bail!("server loop unexpectedly ended")},
        response=transfer.send_request(decode(&auth)?)=>response?,
    };
    let DataSyncResponse::Error { msg } = response else {
        anyhow::bail!("actual uncertain readback needs error")
    };
    assert!(
        !msg.contains("PRIVATE-NOOP-BACKEND-SENTINEL"),
        "actual queued Noop error exposed backend text"
    );
    assert!(msg.contains("DeviceTransportNoopUnavailable"));
    assert!(repo.hits.load(Ordering::SeqCst) > 0);
    Ok(())
}
