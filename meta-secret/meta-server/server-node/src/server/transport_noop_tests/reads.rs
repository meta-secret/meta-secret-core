//! Existing decoded public read routes must reject BEFORE arbitrary Generic traversal.
use super::fixture::*;
use anyhow::Result;
use meta_secret_core::node::{
    api::*,
    db::{
        descriptors::{shared_secret_descriptor::SsLogDescriptor, vault_descriptor::*},
        events::{generic_log_event::*, object_id::*},
    },
    security::sign_event,
};
enum Route {
    Status,
    Log,
    Vault,
    Ss,
    DirectStatus,
    DirectSs,
    OutsiderStatus,
}
async fn malicious_tail(route: Route) -> Result<()> {
    let h = H::new().await?;
    let private = h.private().await?;
    let id = private.obj_id();
    let actor = if matches!(route, Route::OutsiderStatus) {
        &h.outsider
    } else {
        &h.a
    };
    let mut tail = h.tail(actor);
    match route {
        Route::Log => tail.vault_log = id.clone(),
        Route::Vault => tail.vault = id.clone(),
        _ => tail.vault_status = id.clone(),
    }
    h.p.repo.clear_reads();
    let before = h.p.repo.inner.get_db().await;
    let request = VaultRequest {
        sender: actor.user(),
        tail,
    };
    let result = match route {
        Route::Ss => h
            .server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                SsRequest {
                    sender: actor.user(),
                    ss_log: id.clone(),
                },
            ))))
            .await
            .map(|_| ()),
        Route::DirectSs => h
            .gateway()
            .ss_replication(
                SsRequest {
                    sender: actor.user(),
                    ss_log: id.clone(),
                },
                h.a.device_id().clone(),
            )
            .await
            .map(|_| ()),
        Route::DirectStatus => h.gateway().vault_replication(request).await.map(|_| ()),
        _ => h
            .server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(request))))
            .await
            .map(|_| ()),
    };
    let read_count = h.p.repo.read_count(&id);
    eprintln!(
        "PUBLIC_TAIL_GUARD private_reads={read_count} rejected={}",
        result.is_err()
    );
    assert_eq!(
        read_count, 0,
        "RED: forbidden tail must be rejected before affected-Vault discovery/Generic traversal"
    );
    assert!(result.is_err());
    assert!(before == h.p.repo.inner.get_db().await);
    Ok(())
}
macro_rules! attack {
    ($name:ident,$route:ident) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            malicious_tail(Route::$route).await
        }
    };
}
attack!(vault_status_cannot_address_local_private_record, Status);
attack!(vault_log_cannot_address_local_private_record, Log);
attack!(vault_data_cannot_address_local_private_record, Vault);
attack!(ss_log_cannot_address_local_private_record, Ss);
attack!(
    direct_gateway_status_rejects_before_generic_traversal,
    DirectStatus
);
attack!(direct_gateway_ss_rejects_before_generic_traversal, DirectSs);
attack!(
    outsider_status_branch_cannot_address_member_private_record,
    OutsiderStatus
);
#[tokio::test]
async fn legitimate_canonical_vault_and_ss_reads_remain_supported() -> Result<()> {
    let h = H::new().await?;
    let response = h
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            VaultRequest {
                sender: h.a.user(),
                tail: h.tail(&h.a),
            },
        ))))
        .await?;
    let events = response.to_data()?.0;
    assert!(!events.is_empty());
    assert!(events.into_iter().all(|e| matches!(
        e,
        GenericKvLogEvent::Vault(_)
            | GenericKvLogEvent::VaultLog(_)
            | GenericKvLogEvent::VaultStatus(_)
    )));
    let response = h
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
            SsRequest {
                sender: h.a.user(),
                ss_log: ArtifactId::from(SsLogDescriptor::from(h.a.vault_name.clone())),
            },
        ))))
        .await?;
    assert!(response.to_data()?.0.into_iter().all(|e| matches!(
        e,
        GenericKvLogEvent::SsLog(_) | GenericKvLogEvent::SsWorkflow(_)
    )));
    Ok(())
}
#[tokio::test]
async fn generic_signed_local_private_record_cannot_reach_network_persistence() -> Result<()> {
    let h = H::new().await?;
    let record = h.private().await?;
    let signed = sign_event(
        record.to_generic(),
        h.a.device_id().clone(),
        &h.a.device_creds.key_manager()?.dsa,
    )?;
    let before = h.p.repo.inner.get_db().await;
    h.p.repo.clear_reads();
    assert!(
        h.server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                signed
            ))))
            .await
            .is_err()
    );
    assert!(before == h.p.repo.inner.get_db().await);
    Ok(())
}
#[tokio::test]
async fn other_device_status_tail_is_rejected_before_its_record_read() -> Result<()> {
    let h = H::new().await?;
    let id = ArtifactId::from(VaultStatusDescriptor::from(h.b.user_id()));
    let mut tail = h.tail(&h.a);
    tail.vault_status = id.clone();
    h.p.repo.clear_reads();
    let result = h
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            VaultRequest {
                sender: h.a.user(),
                tail,
            },
        ))))
        .await;
    assert_eq!(
        h.p.repo.read_count(&id),
        0,
        "RED: caller cannot traverse a different device's status stream"
    );
    assert!(result.is_err());
    Ok(())
}
