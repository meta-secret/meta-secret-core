use crate::server::server_app::ServerApp;
use anyhow::Result;
use meta_secret_core::node::api::{DataSyncResponse, SyncRequest};
use meta_secret_core::node::app::sync::sync_protocol::SyncProtocol;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use std::sync::Arc;

pub struct EmbeddedSyncProtocol {
    pub server: Arc<ServerApp<InMemKvLogEventRepo>>,
}

impl SyncProtocol for EmbeddedSyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        self.server.handle_client_request(request).await
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use crate::server::server_app::ServerApp;
    use crate::server::server_sync_protocol::EmbeddedSyncProtocol;
    use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
    use std::sync::Arc;

    pub struct SyncProtocolFixture {
        pub sync_protocol: Arc<EmbeddedSyncProtocol>,
    }

    impl SyncProtocolFixture {
        pub fn new(server: Arc<ServerApp<InMemKvLogEventRepo>>) -> SyncProtocolFixture {
            let sync_protocol = EmbeddedSyncProtocol { server };

            SyncProtocolFixture {
                sync_protocol: Arc::new(sync_protocol),
            }
        }
    }
}
