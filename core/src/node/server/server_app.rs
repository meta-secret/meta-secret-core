use std::sync::Arc;

use anyhow::Result;
use tracing::{error, info, instrument};

use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::device::device_creds::DeviceCredentials;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::objects::global_index::ServerPersistentGlobalIndex;
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::repo::credentials_repo::CredentialsRepo;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::SyncRequest;
use crate::node::server::server_data_sync::{
    DataEventsResponse, DataSyncApi, DataSyncRequest, DataSyncResponse, ServerDataSync, ServerTailResponse,
};

pub struct ServerApp<Repo: KvLogEventRepo> {
    pub data_sync: ServerDataSync<Repo>,
    pub p_obj: Arc<PersistentObject<Repo>>,
    creds_repo: CredentialsRepo<Repo>,
    server_dt: Arc<ServerDataTransfer>,
}

pub struct ServerDataTransfer {
    pub dt: MpscDataTransfer<DataSyncRequest, DataSyncResponse>,
}

impl<Repo: KvLogEventRepo> ServerApp<Repo> {
    pub fn new(repo: Arc<Repo>, server_dt: Arc<ServerDataTransfer>) -> Result<Self> {
        let p_obj = {
            let obj = PersistentObject::new(repo);
            Arc::new(obj)
        };

        let data_sync = ServerDataSync {
            persistent_obj: p_obj.clone(),
        };

        let creds_repo = CredentialsRepo { p_obj: p_obj.clone() };

        Ok(Self { data_sync, p_obj, creds_repo, server_dt })
    }

    async fn init(&self) -> Result<DeviceCredentials> {
        let device_creds = self.get_creds().await?;

        let gi_obj = ServerPersistentGlobalIndex {
            p_obj: self.data_sync.persistent_obj.clone(),
            server_device: device_creds.device.clone(),
        };

        gi_obj.init().await?;

        Ok(device_creds)
    }

    #[instrument(skip_all)]
    pub async fn run(&self) -> Result<()> {
        info!("Run server app");

        let _ = self.init().await?;

        while let Ok(sync_message) = self.server_dt.dt.service_receive().await {
            self.handle_client_request(sync_message).await?;
        }

        Ok(())
    }

    #[instrument(skip_all)]
    async fn handle_client_request(&self, sync_message: DataSyncRequest) -> Result<()> {
        match sync_message {
            DataSyncRequest::SyncRequest(request) => {
                let new_events = self.handle_sync_request(request).await;

                self.server_dt
                    .dt
                    .send_to_client(DataSyncResponse::Data(DataEventsResponse(new_events)))
                    .await;
            }
            DataSyncRequest::Event(event) => {
                self.handle_new_event(event).await?;
            }
            DataSyncRequest::ServerTailRequest(user_id) => {
                let p_device_log = PersistentDeviceLog {
                    p_obj: self.p_obj.clone(),
                };
                let device_log_tail = p_device_log.find_tail_id(&user_id).await?;

                let p_ss = PersistentSharedSecret {
                    p_obj: self.p_obj.clone(),
                };
                let ss_device_log_tail = p_ss.find_device_tail_id(&user_id.device_id).await?;

                let response = ServerTailResponse {
                    device_log_tail,
                    ss_device_log_tail,
                };
                let response = DataSyncResponse::ServerTailResponse(response);

                self.server_dt.dt.send_to_client(response).await;
            }
        }
        Ok(())
    }

    async fn handle_new_event(&self, event: GenericKvLogEvent) -> Result<()> {
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
        self.creds_repo
            .get_or_generate_device_creds(DeviceName::server())
            .await
    }
}

#[cfg(test)]
pub mod fixture {
    use std::sync::Arc;
    use crate::node::common::data_transfer::MpscDataTransfer;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::fixture::PersistentObjectFixture;
    use crate::node::server::server_app::{ServerApp, ServerDataTransfer};

    pub struct ServerAppFixture {
        pub server_app: ServerApp<InMemKvLogEventRepo>,
    }

    impl ServerAppFixture {
        pub async fn try_from(
            p_obj: &PersistentObjectFixture, server_dt_fxr: ServerDataTransferFixture,
        ) -> anyhow::Result<Self> {
            let server_app = ServerApp::new(p_obj.server.repo.clone(), server_dt_fxr.server_dt)?;
            Ok(Self { server_app })
        }
    }

    pub struct ServerDataTransferFixture {
        pub server_dt: Arc<ServerDataTransfer>,
    }

    impl ServerDataTransferFixture {
        pub fn generate() -> Self {
            let server_dt = Arc::new(ServerDataTransfer { dt: MpscDataTransfer::new() });

            Self { server_dt }
        }
    }
}
