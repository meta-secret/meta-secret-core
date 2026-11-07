//! Existing signed Recovery admission/channel contract; no new permission policy.
use super::super::{recovery_intake_fixture::Publisher, recovery_receipt_fixture::ReceiptHarness};
use super::*;
use meta_secret_core::node::db::events::shared_secret_event::SsWorkflowObject;
use std::sync::atomic::Ordering;

async fn read(corrupt: bool, direct: bool) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let h = ReceiptHarness::new(inner.clone()).await?;
    let publisher = Arc::new(Publisher::default());
    let server = ServerApp::new_with_invalidation_publisher(
        inner.clone(),
        h.base.master_key.clone(),
        publisher.clone(),
    )?;
    server.init().await?;
    let packet = h.packet(&h.base.b, true)?;
    let id = packet.event()?.obj_id();
    send(&server, packet).await?;
    h.assert_winner(true).await?;
    if corrupt {
        let GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(mut actual)) = inner
            .find_one(id.clone())
            .await?
            .context("actually admitted B Recovery")?
        else {
            unreachable!()
        };
        let GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(other)) =
            h.packet(&h.base.c, true)?.event()?.clone()
        else {
            unreachable!()
        };
        // Correct admitted B key/claim/Vault/recipient A, but actual C→A cipher.
        actual.value.secret_message = other.value.secret_message;
        inner
            .db
            .lock()
            .await
            .insert(id.clone(), SsWorkflowObject::Recovery(actual).to_generic());
    }
    let before = inner.get_db().await;
    let published = publisher.0.load(Ordering::SeqCst);
    let SyncRequest::Read(request) = h.ss_request(&h.base.a, None) else {
        unreachable!()
    };
    let ReadSyncRequest::SsRequest(request) = *request else {
        unreachable!()
    };
    let gateway = crate::server::server_data_sync::ServerSyncGateway::new(
        Arc::new(PersistentObject::new(inner.clone())),
        publisher.clone(),
    );
    let result = if direct {
        gateway
            .ss_replication(request, server.init().await?.device.device_id)
            .await
    } else {
        server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                request,
            ))))
            .await
            .map(|response| response.to_data().unwrap().0)
    };
    if corrupt {
        eprintln!(
            "actual signed Recovery reader: direct={direct} db_unchanged={} returned_error={}",
            inner.get_db().await == before,
            result.is_err()
        );
        anyhow::ensure!(
            inner.get_db().await == before,
            "corrupt addressed Recovery was deleted/updated before source/channel validation"
        );
        anyhow::ensure!(
            publisher.0.load(Ordering::SeqCst) == published,
            "corrupt Recovery published invalidation"
        );
        anyhow::ensure!(
            result.is_err(),
            "wrong actual admitted Recovery source/channel was delivered"
        );
    } else {
        let events = result?;
        assert!(
            events.iter().any(|event| event.obj_id() == id),
            "real valid signed Recovery must be delivered"
        );
        assert!(inner.find_one(id).await?.is_none());
        h.assert_winner(true).await?;
    }
    Ok(())
}
#[tokio::test]
async fn noop_b_actual_signed_recovery_delivery_control() -> Result<()> {
    read(false, false).await?;
    read(false, true).await
}
#[tokio::test]
async fn noop_b_addressed_recovery_wrong_admitted_channel_rejects_before_delete() -> Result<()> {
    let outer = read(true, false).await;
    let direct = read(true, true).await;
    outer?;
    direct
}
