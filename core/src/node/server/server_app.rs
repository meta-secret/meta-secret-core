use std::sync::Arc;

use tracing::{error, info, instrument};

use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::DeviceCredentials;
use crate::node::db::descriptors::global_index::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::objects::persistent_object::PersistentGlobalIndexApi;
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

        self.gi_initialization().await;

        while let Ok(sync_message) = self.data_transfer.dt.service_receive().await {
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
                        .send_to_client(DataSyncResponse { events: new_events})
                        .await;
                }
                DataSyncRequest::Event(event) => {
                    self.data_sync.send(event).await?;
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn gi_initialization(&self) {
        //Check if all required persistent objects has been created
        let gi_obj_desc = ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index);

        let maybe_gi_unit_id = {
            let gi_unit = ObjectId::unit(gi_obj_desc.clone());
            self.repo().find_one(gi_unit).await
        };

        let maybe_gi_genesis = self.repo()
            .find_one(ObjectId::genesis(gi_obj_desc))
            .await;

        let gi_genesis_exists = matches!(maybe_gi_genesis, Ok(Some(_)));
        let gi_unit_exists = matches!(maybe_gi_unit_id, Ok(Some(_)));

        //If either of unit or genesis not exists then create initial records for the global index
        if !gi_unit_exists || !gi_genesis_exists {
            let server_pk = self.device_creds.device.clone();
            let _meta_g = self
                .data_sync
                .persistent_obj
                .global_index
                .gi_init(server_pk)
                .await;
        }
    }
    fn repo(&self) -> Arc<Repo> {
        self.data_sync.persistent_obj.repo.clone()
    }
}
