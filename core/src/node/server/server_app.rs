use std::sync::Arc;

use tracing::{error, info, instrument};

use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::{DeviceCredentials, DeviceData};
use crate::node::db::objects::global_index::PersistentGlobalIndex;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::data_sync::{DataSyncApi, DataSyncRequest, DataSyncResponse, ServerDataSync};

pub struct ServerApp<Repo: KvLogEventRepo> {
    pub data_sync: ServerDataSync<Repo>,
    pub data_transfer: Arc<ServerDataTransfer>,
    pub device_creds: DeviceCredentials
}

pub struct ServerDataTransfer {
    pub dt: MpscDataTransfer<DataSyncRequest, DataSyncResponse>,
}

impl<Repo: KvLogEventRepo> ServerApp<Repo> {

    #[instrument(skip(self))]
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Run server app");

        let gi_obj = PersistentGlobalIndex {
            p_obj: self.data_sync.persistent_obj.clone()
        };

        gi_obj.init(self.device_creds.device.clone()).await?;

        while let Ok(sync_message) = self.data_transfer.dt.service_receive().await {
            self.handle_sync_request(sync_message).await?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn handle_sync_request(&self, sync_message: DataSyncRequest) -> anyhow::Result<()> {
        match sync_message {
            DataSyncRequest::SyncRequest(request) => {
                let new_events_result = self.data_sync
                    .replication(request)
                    .await;

                let new_events = match new_events_result {
                    Ok(data) => {
                        //debug!(format!("New events for a client: {:?}", data).as_str());
                        data
                    }
                    Err(_) => {
                        error!("Server. Sync Error");
                        vec![]
                    }
                };

                self.data_transfer.dt
                    .send_to_client(DataSyncResponse { events: new_events })
                    .await;
            }
            DataSyncRequest::Event(event) => {
                self.data_sync.send(event).await?;
            }
        }
        Ok(())
    }

    fn repo(&self) -> Arc<Repo> {
        self.data_sync.persistent_obj.repo.clone()
    }
}

#[cfg(test)]
mod test {

    #[tokio::test]
    async fn test_global_index_initialization() {

    }
}
