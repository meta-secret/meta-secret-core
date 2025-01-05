use crate::node::common::model::device::common::DeviceData;
use crate::node::db::descriptors::global_index_descriptor::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::objects::global_index::ClientPersistentGlobalIndex;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::GlobalIndexRequest;
use anyhow::Result;
use std::sync::Arc;

pub struct GlobalIndexDbSync<Repo: KvLogEventRepo> {
    p_obj: Arc<PersistentObject<Repo>>,
    sender: DeviceData,
    p_gi: ClientPersistentGlobalIndex<Repo>,
}

impl<Repo: KvLogEventRepo> GlobalIndexDbSync<Repo> {
    pub fn new(p_obj: Arc<PersistentObject<Repo>>, sender: DeviceData) -> Self {
        let p_gi = ClientPersistentGlobalIndex {
            p_obj: p_obj.clone(),
        };

        Self {
            p_obj,
            sender,
            p_gi,
        }
    }
}

impl<Repo: KvLogEventRepo> GlobalIndexDbSync<Repo> {
    pub async fn save(&self, gi_events: Vec<GenericKvLogEvent>) -> Result<()> {
        for gi_event in gi_events {
            let gi_obj = gi_event.global_index()?;
            self.p_gi.save(&gi_obj).await?;
        }

        Ok(())
    }
}

pub struct GlobalIndexDbSyncRequest<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub sender: DeviceData,
}

impl<Repo: KvLogEventRepo> GlobalIndexDbSyncRequest<Repo> {
    const GI_DESC: ObjectDescriptor = ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index);

    /// Get a free global index id, to sync from
    pub async fn get(&self) -> Result<GlobalIndexRequest> {
        let gi_free_id = self.p_obj.find_free_id_by_obj_desc(Self::GI_DESC).await?;

        Ok(GlobalIndexRequest {
            sender: self.sender.clone(),
            global_index: gi_free_id,
        })
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::app::sync::global_index::GlobalIndexDbSyncRequest;
    use crate::node::db::descriptors::global_index_descriptor::GlobalIndexDescriptor;
    use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
    use crate::node::db::events::object_id::ObjectId;
    use anyhow::Result;

    #[tokio::test]
    async fn test_gi_request() -> Result<()> {
        let fixture = FixtureRegistry::empty();

        let db_sync_request = GlobalIndexDbSyncRequest {
            p_obj: fixture.state.p_obj.client.clone(),
            sender: fixture.state.device_creds.client.device,
        };

        let sync = db_sync_request.get().await?;

        let expected_id = ObjectId::unit(GlobalIndexDescriptor::Index.to_obj_desc());
        assert_eq!(expected_id, sync.global_index);

        Ok(())
    }
}
