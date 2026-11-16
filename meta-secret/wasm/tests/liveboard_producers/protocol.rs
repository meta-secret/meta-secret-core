//! Every business request is delegated to the actual signed ordered ServerApp.
use anyhow::Result;
use meta_secret_core::node::{
    api::*, app::sync::sync_protocol::SyncProtocol, db::in_mem_db::InMemKvLogEventRepo,
};
use meta_server_node::server::server_app::ServerApp;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub struct Network {
    pub server: Arc<ServerApp<InMemKvLogEventRepo>>,
    pub repo: Arc<InMemKvLogEventRepo>,
    pub writes: AtomicUsize,
    pub observations: AtomicUsize,
    pub ordinary_reads: AtomicUsize,
}
impl Network {
    pub fn new(
        server: Arc<ServerApp<InMemKvLogEventRepo>>,
        repo: Arc<InMemKvLogEventRepo>,
    ) -> Self {
        Self {
            server,
            repo,
            writes: AtomicUsize::new(0),
            observations: AtomicUsize::new(0),
            ordinary_reads: AtomicUsize::new(0),
        }
    }
    pub fn counts(&self) -> (usize, usize, usize) {
        (
            self.writes.load(Ordering::SeqCst),
            self.observations.load(Ordering::SeqCst),
            self.ordinary_reads.load(Ordering::SeqCst),
        )
    }
}
impl SyncProtocol for Network {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        match &request {
            SyncRequest::Write(_) => {
                self.writes.fetch_add(1, Ordering::SeqCst);
            }
            SyncRequest::Read(read) => match read.as_ref() {
                ReadSyncRequest::ServerTail(query) if query.producer_reference.is_some() => {
                    self.observations.fetch_add(1, Ordering::SeqCst);
                }
                _ => {
                    self.ordinary_reads.fetch_add(1, Ordering::SeqCst);
                }
            },
        }
        Box::pin(self.server.handle_client_request(request)).await
    }
}
