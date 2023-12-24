mod common;

#[cfg(test)]
mod test {
    use meta_secret_core::node::{
        server::{
            request::{GlobalIndexRequest, SyncRequest},
            server_app::ServerApp
        },
        db::{
            descriptors::global_index::GlobalIndexDescriptor,
            descriptors::object_descriptor::ToObjectDescriptor,
            events::object_id::ObjectId,
            in_mem_db::InMemKvLogEventRepo
        }
    };
    use std::sync::Arc;
    use meta_secret_core::crypto::keys::{KeyManager, OpenBox};
    use meta_secret_core::node::common::model::device::{DeviceData, DeviceName};


    #[tokio::test]
    pub async fn test_server_app() -> anyhow::Result<()> {
        let repo = Arc::new(InMemKvLogEventRepo::default());

        let server_app = ServerApp::init(repo.clone()).await?;

        let client_device = {
            let secret_box = KeyManager::generate_secret_box();
            let open_box = OpenBox::from(&secret_box);
            DeviceData::from(DeviceName::from("test_device"), open_box)
        };

        let sync_request = SyncRequest::GlobalIndex(GlobalIndexRequest {
            sender: client_device,
            global_index: ObjectId::unit(GlobalIndexDescriptor::Index.to_obj_desc())
        });

        let events = server_app
            .handle_sync_request(sync_request)
            .await;

        for event in events {
            println!("Event: {}", serde_json::to_string(&event).unwrap());
        }

        Ok(())
    }
}
