use crate::node::db::in_mem_db::InMemKvLogEventRepo;
use crate::node::api::SyncRequest;
use crate::node::server::server_app::ServerApp;
use crate::node::server::server_data_sync::DataSyncResponse;
use anyhow::Result;
use std::sync::Arc;

pub trait SyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse>;
}

pub struct EmbeddedSyncProtocol {
    pub server: Arc<ServerApp<InMemKvLogEventRepo>>,
}

impl SyncProtocol for EmbeddedSyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        self.server.handle_client_request(request).await
    }
}

#[cfg(test)]
pub mod fixture {
    use crate::node::app::sync::sync_protocol::EmbeddedSyncProtocol;
    use crate::node::server::server_app::fixture::ServerAppFixture;
    use std::sync::Arc;

    pub struct SyncProtocolFixture {
        pub sync_protocol: Arc<EmbeddedSyncProtocol>,
    }

    impl SyncProtocolFixture {
        pub fn new(server_app_fixture: Arc<ServerAppFixture>) -> SyncProtocolFixture {
            let sync_protocol = EmbeddedSyncProtocol {
                server: server_app_fixture.server_app.clone(),
            };

            SyncProtocolFixture {
                sync_protocol: Arc::new(sync_protocol),
            }
        }
    }
}
