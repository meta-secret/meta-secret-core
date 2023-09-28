use std::sync::Arc;
use tracing::{error, info};

use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::db::events::common::ObjectCreator;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_service::MetaDbServiceProxy;
use crate::node::db::objects::persistent_object::PersistentGlobalIndexApi;

use crate::node::server::data_sync::{DataSyncApi, DataSyncMessage, MetaServerContext, ServerDataSync};

pub struct ServerApp<Repo: KvLogEventRepo> {
    pub data_sync: Arc<ServerDataSync<Repo>>,
    pub data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
    pub meta_db_service_proxy: Arc<MetaDbServiceProxy>,
}

impl<Repo> ServerApp<Repo>
where
    Repo: KvLogEventRepo,
{
    pub async fn run(&self) {
        info!("Run server app");

        self.gi_initialization().await;

        while let Ok(sync_message) = self.data_transfer.service_receive().await {
            match sync_message {
                DataSyncMessage::SyncRequest(request) => {
                    self.meta_db_service_proxy.sync_db().await;

                    let new_events_result = self.data_sync.replication(request).await;
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

                    self.data_transfer.send_to_client(new_events).await;
                }
                DataSyncMessage::Event(event) => {
                    self.data_sync.send(event).await;
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
            .await;

        let maybe_gi_genesis = self
            .data_sync
            .persistent_obj
            .repo
            .find_one(ObjectId::genesis(&ObjectDescriptor::GlobalIndex))
            .await;

        let gi_genesis_exists = matches!(maybe_gi_genesis, Ok(Some(_)));
        let gi_unit_exists = matches!(maybe_gi_unit_id, Ok(Some(_)));

        //If either of unit or genesis not exists then create initial records for the global index
        if !gi_unit_exists || !gi_genesis_exists {
            let server_pk = self.data_sync.context.server_pk();
            let _meta_g = self.data_sync.persistent_obj.global_index.gi_init(&server_pk).await;
        }
    }
}
