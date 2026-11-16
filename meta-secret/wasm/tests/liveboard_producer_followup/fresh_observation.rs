//! Old Applied/raw-ACK facts never replace one fresh successful completion observation.
use super::{
    context_repo::{Fault, Repo},
    fixture::*,
    protocol::Network,
    terminal::{context, interrupt},
};
use anyhow::{Result, bail};
use meta_secret_core::node::{
    api::{DataSyncResponse, ReadSyncRequest, SyncRequest},
    app::{
        meta_app::{
            client_transfer::MetaClientRequestTransfer,
            meta_client_service::{
                MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
            },
        },
        sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol},
    },
    common::model::liveboard_producer::*,
    db::in_mem_db::InMemKvLogEventRepo,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct ProbeWire {
    real: Arc<Network>,
    fail_query: AtomicBool,
    attempts: AtomicUsize,
}
impl SyncProtocol for ProbeWire {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        if matches!(&request, SyncRequest::Read(r)
            if matches!(r.as_ref(), ReadSyncRequest::ServerTail(q) if q.producer_reference.is_some()))
        {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail_query.load(Ordering::SeqCst) {
                bail!("PRIVATE-FOLLOWUP real atomic observation transport unavailable");
            }
        }
        self.real.send(request).await
    }
}

#[tokio::test]
async fn failed_fresh_atomic_query_cannot_clear_old_applied_context_then_healthy_query_completes_same_u()
-> Result<()> {
    let backend = Arc::new(InMemKvLogEventRepo::default());
    let saved = interrupt(backend.clone(), Fault::ClearBefore).await?;
    let original = saved.receipt.clone();
    let inner = Arc::new(Repo::new(backend));
    let h = Harness::reopen(inner.clone(), saved.state)?;
    let wire = Arc::new(ProbeWire {
        real: h.network.clone(),
        fail_query: AtomicBool::new(true),
        attempts: AtomicUsize::new(0),
    });
    let gateway = Arc::new(SyncGateway::new(
        "actual-fresh-query-boundary".into(),
        h.p.clone(),
        wire.clone(),
        h.master.clone(),
    ));
    let service = Arc::new(MetaClientService {
        data_transfer: Arc::new(MetaClientDataTransfer {
            dt: MetaClientRequestTransfer::new(),
        }),
        sync_gateway: gateway.clone(),
        state_provider: Arc::new(MetaClientStateProvider::new()),
        p_obj: h.p.clone(),
        device_data: h.actor.device(),
        master_key: h.master.clone(),
    });
    let web = meta_secret_wasm::app_manager::ApplicationManager::new(
        wire.clone(),
        gateway,
        service.clone(),
        h.master.clone(),
    );
    let scenario = async {
        let before = context(&h).await?;
        assert!(before.as_ref() == Some(&saved.context));
        inner.reset();
        h.p.repo.reset_observation();
        let business = h.network.counts().0;
        let result = web
            .continue_local_remove_checked(reference(&original))
            .await;
        if let Ok(receipt) = result {
            assert_eq!(receipt.phase, ProducerPhase::Unknown);
            assert!(!["ready", "done", "settled"].contains(&receipt.readiness.as_str()));
        }
        assert!(
            context(&h).await? == before,
            "failed fresh query cannot clear/unlock a context using old acknowledged facts"
        );
        assert_eq!(inner.writes.load(Ordering::SeqCst), 0);
        assert_eq!(inner.material_writes.load(Ordering::SeqCst), 0);
        assert!(h.p.repo.sources().is_empty());
        assert_eq!(h.network.counts().0, business);
        assert_eq!(
            wire.attempts.load(Ordering::SeqCst),
            1,
            "RED: exact completion continuation must attempt one mandatory fresh atomic observation"
        );
        wire.fail_query.store(false, Ordering::SeqCst);
        let completed = web
            .continue_local_remove_checked(reference(&original))
            .await?;
        assert!(context(&h).await?.is_none());
        assert_eq!(completed.phase, ProducerPhase::Queued);
        assert_eq!(completed.canonical_state, ProducerCanonicalState::Applied);
        assert_eq!(completed.request_id, original.request_id);
        assert_eq!(
            completed.request_stage.source,
            original.request_stage.source
        );
        assert_eq!(completed.decision_stage, original.decision_stage);
        assert_eq!(inner.material_writes.load(Ordering::SeqCst), 0);
        assert!(h.p.repo.sources().is_empty());
        assert_eq!(h.network.counts().0, business);
        Ok(())
    };
    tokio::select! {
        result=scenario=>result,
        result=Box::pin(service.run())=>{result?;bail!("real producer service ended before scenario")},
        _=tokio::time::sleep(std::time::Duration::from_secs(10))=>bail!("watchdog; no effect inferred from elapsed time"),
    }
}
