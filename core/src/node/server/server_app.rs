use std::sync::Arc;

use tracing::{error, info, instrument};
use anyhow::Result;

use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::{DeviceName, DeviceCredentials};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::objects::global_index::PersistentGlobalIndex;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::credentials_repo::CredentialsRepo;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::data_sync::{DataSyncApi, DataSyncRequest, DataSyncResponse, ServerDataSync};
use crate::node::server::request::SyncRequest;

pub struct ServerApp<Repo: KvLogEventRepo> {
    data_sync: ServerDataSync<Repo>,
    p_obj: Arc<PersistentObject<Repo>>
}

pub struct ServerDataTransfer {
    pub dt: MpscDataTransfer<DataSyncRequest, DataSyncResponse>,
}

impl<Repo: KvLogEventRepo> ServerApp<Repo> {
    pub async fn init(repo: Arc<Repo>) -> anyhow::Result<Self> {
        let p_obj = {
            let obj = PersistentObject::new(repo);
            Arc::new(obj)
        };

        let creds_repo = CredentialsRepo { p_obj: p_obj.clone() };

        let device_creds = creds_repo
            .get_or_generate_device_creds(DeviceName::from("server"))
            .await?;

        let data_sync = ServerDataSync {
            persistent_obj: p_obj.clone(),
            device_creds: device_creds.clone(),
        };

        let gi_obj = PersistentGlobalIndex {
            p_obj: data_sync.persistent_obj.clone(),
            server_device: device_creds.device.clone(),
        };

        gi_obj.init().await?;

        Ok(ServerApp { data_sync, p_obj })
    }

    #[instrument(skip_all)]
    pub async fn run(&self, data_transfer: Arc<ServerDataTransfer>) -> anyhow::Result<()> {
        info!("Run server app");

        while let Ok(sync_message) = data_transfer.dt.service_receive().await {
            self.handle_client_request(sync_message, data_transfer.clone()).await?;
        }

        Ok(())
    }

    #[instrument(skip_all)]
    async fn handle_client_request(
        &self,
        sync_message: DataSyncRequest,
        data_transfer: Arc<ServerDataTransfer>,
    ) -> anyhow::Result<()> {
        match sync_message {
            DataSyncRequest::SyncRequest(request) => {
                let new_events = self.handle_sync_request(request).await;

                data_transfer
                    .dt
                    .send_to_client(DataSyncResponse { events: new_events })
                    .await;
            }
            DataSyncRequest::Event(event) => {
                self.handle_new_event(event).await?;
            }
        }
        Ok(())
    }

    async fn handle_new_event(&self, event: GenericKvLogEvent) -> anyhow::Result<()> {
        self.data_sync.send(event).await?;
        Ok(())
    }

    pub async fn handle_sync_request(&self, request: SyncRequest) -> Vec<GenericKvLogEvent> {
        let new_events_result = self.data_sync.replication(request).await;

        match new_events_result {
            Ok(data) => {
                //debug!(format!("New events for a client: {:?}", data).as_str());
                data
            }
            Err(_) => {
                error!("Server. Sync Error");
                vec![]
            }
        }
    }

    pub async fn get_creds(&self) -> Result<DeviceCredentials> {
        let creds_repo = CredentialsRepo { p_obj: self.p_obj.clone() };

        creds_repo
            .get_or_generate_device_creds(DeviceName::from("server"))
            .await
    }
}
