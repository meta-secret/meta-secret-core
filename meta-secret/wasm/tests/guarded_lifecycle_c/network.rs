use super::fixture::Snapshot;
use anyhow::{Context, Result, bail, ensure};
use meta_secret_core::node::{
    api::*, app::sync::sync_protocol::SyncProtocol, db::in_mem_db::InMemKvLogEventRepo,
};
use meta_server_node::server::server_app::ServerApp;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, AtomicUsize, Ordering},
};

#[derive(Clone, Copy)]
pub enum Capability {
    Actual = 0,
    NoopAbsent = 1,
    TransportFailure = 2,
}
pub struct Network {
    pub server: Arc<ServerApp<InMemKvLogEventRepo>>,
    local: Arc<InMemKvLogEventRepo>,
    mode: AtomicU8,
    pub probes: AtomicUsize,
    pub writes: AtomicUsize,
    pub accepts: Mutex<Vec<Snapshot>>,
}
impl Network {
    pub fn new(
        server: Arc<ServerApp<InMemKvLogEventRepo>>,
        local: Arc<InMemKvLogEventRepo>,
    ) -> Self {
        Self {
            server,
            local,
            mode: AtomicU8::new(Capability::Actual as u8),
            probes: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
            accepts: Mutex::new(vec![]),
        }
    }
    pub fn arm(&self, mode: Capability) {
        self.mode.store(mode as u8, Ordering::SeqCst);
    }
}
impl SyncProtocol for Network {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        if matches!(&request, SyncRequest::Read(read) if matches!(&**read, ReadSyncRequest::LiveboardCapabilities(_)))
        {
            self.probes.fetch_add(1, Ordering::SeqCst);
            if self.mode.load(Ordering::SeqCst) == Capability::TransportFailure as u8 {
                bail!("C1 injected capability transport unavailable");
            }
            let response = Box::pin(self.server.handle_client_request(request)).await?;
            let DataSyncResponse::LiveboardCapabilities(mut capabilities) = response else {
                bail!("actual server omitted capabilities");
            };
            ensure!(
                capabilities.projection_version == 1
                    && capabilities.guarded_removal_version == 1
                    && capabilities.device_transport_noop_version == 1,
                "actual current server capability control not reached"
            );
            if self.mode.load(Ordering::SeqCst) == Capability::NoopAbsent as u8 {
                capabilities.device_transport_noop_version = 0;
            }
            return Ok(DataSyncResponse::LiveboardCapabilities(capabilities));
        }
        if let SyncRequest::Write(write) = &request {
            self.writes.fetch_add(1, Ordering::SeqCst);
            if let WriteSyncRequest::Event(action) = &**write
                && let SignedActionPayload::GuardedRemovalDecision(guard) = &action.payload
                && matches!(&guard.event.0.value, meta_secret_core::node::db::events::vault::vault_log_event::VaultActionEvent::Update(meta_secret_core::node::db::events::vault::vault_log_event::VaultActionUpdateEvent::RemoveMember(update)) if update.decision == meta_secret_core::node::db::events::vault::vault_log_event::RemoveMemberDecision::Accepted)
            {
                let snapshot = self.local.get_db().await;
                self.accepts.lock().unwrap().push(snapshot);
            }
        }
        Box::pin(self.server.handle_client_request(request))
            .await
            .context("actual C1 ServerApp transport")
    }
}
