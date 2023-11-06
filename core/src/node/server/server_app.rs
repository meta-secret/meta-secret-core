use std::sync::Arc;
use tracing::{error, info, instrument, Instrument};

use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::DeviceCredentials;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentGlobalIndexApi;

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
    pub async fn run(&self) {
        info!("Run server app");

        self.gi_initialization().in_current_span().await;

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
                        .send_to_client(DataSyncResponse::Data { events: new_events})
                        .await;
                }
                DataSyncRequest::Event(event) => {
                    self.data_sync.send(event).in_current_span().await;
                }
            }
        }
    }

    async fn gi_initialization(&self) {
        //Check if all required persistent objects has been created
        let maybe_gi_unit_id = self
            .data_sync
            .persistent_obj
            .repo
            .find_one(ObjectId::unit(&ObjectDescriptor::GlobalIndex))
            .in_current_span()
            .await;

        let maybe_gi_genesis = self
            .data_sync
            .persistent_obj
            .repo
            .find_one(ObjectId::genesis(&ObjectDescriptor::GlobalIndex))
            .in_current_span()
            .await;

        let gi_genesis_exists = matches!(maybe_gi_genesis, Ok(Some(_)));
        let gi_unit_exists = matches!(maybe_gi_unit_id, Ok(Some(_)));

        //If either of unit or genesis not exists then create initial records for the global index
        if !gi_unit_exists || !gi_genesis_exists {
            let server_pk = PublicKeyRecord::from(self.device_creds.secret_box.dsa.public_key.clone());
            let _meta_g = self
                .data_sync
                .persistent_obj
                .global_index
                .gi_init(&server_pk)
                .in_current_span()
                .await;
        }
    }
}
