use std::sync::Arc;

use crate::node::{server::{request::{SyncRequest, GlobalIndexRequest}, server_app::ServerApp}, common::model::device::{DeviceData, DeviceCredentials}, db::{events::{generic_log_event::GenericKvLogEvent, object_id::ObjectId}, objects::persistent_object::PersistentObject, in_mem_db::InMemKvLogEventRepo, descriptors::{global_index::GlobalIndexDescriptor, object_descriptor::ToObjectDescriptor}}};

pub struct ServerTestNode {
    pub device: DeviceCredentials,
    pub p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,
    pub app: ServerApp<InMemKvLogEventRepo>,
}

impl ServerTestNode {
    pub async fn new() -> anyhow::Result<Self> {
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));

        let app = ServerApp::init(repo).await?;
        let device = app.get_creds().await?;

        Ok(Self { p_obj, app, device })
    }
}

pub struct GlobalIndexSyncRequestTestAction {
    pub server_node: ServerTestNode,
}

impl GlobalIndexSyncRequestTestAction {
    pub async fn init() -> anyhow::Result<Self> {
        let server_node = ServerTestNode::new().await?;
        Ok(Self { server_node })
    }
}

impl GlobalIndexSyncRequestTestAction {
    pub async fn send_request(&self, client_device: DeviceData) -> Vec<GenericKvLogEvent> {
        let sync_request = SyncRequest::GlobalIndex(GlobalIndexRequest {
            sender: client_device,
            global_index: ObjectId::unit(GlobalIndexDescriptor::Index.to_obj_desc()),
        });

        self.server_node.app.handle_sync_request(sync_request).await
    }
}
