use crate::node::common::model::device::DeviceData;
use crate::node::db::descriptors::global_index_descriptor::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::global_index_event::GlobalIndexObject;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use std::sync::Arc;
use tracing::info;
use tracing_attributes::instrument;

pub struct PersistentGlobalIndex<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub server_device: DeviceData,
}

impl<Repo: KvLogEventRepo> PersistentGlobalIndex<Repo> {
    ///create a genesis event and save into the database
    #[instrument(skip(self))]
    pub async fn init(&self) -> anyhow::Result<()> {
        //Check if all required persistent objects has been created
        let gi_obj_desc = GlobalIndexDescriptor::Index.to_obj_desc();

        let maybe_unit_event = {
            let gi_unit = ObjectId::unit(gi_obj_desc.clone());
            self.p_obj.repo.find_one(gi_unit).await
        };

        let maybe_genesis_event = self.p_obj.repo.find_one(ObjectId::genesis(gi_obj_desc)).await;

        let gi_genesis_exists = matches!(maybe_unit_event, Ok(Some(_)));
        let gi_unit_exists = matches!(maybe_genesis_event, Ok(Some(_)));

        //If either of unit or genesis not exists then create initial records for the global index
        if gi_unit_exists && gi_genesis_exists {
            return Ok(());
        }

        info!("Init global index");

        let unit_event = GlobalIndexObject::Unit(KvLogEvent::global_index_unit()).to_generic();
        let genesis_event =
            GlobalIndexObject::Genesis(KvLogEvent::global_index_genesis(self.server_device.clone())).to_generic();

        self.p_obj.repo.save(unit_event.clone()).await?;
        self.p_obj.repo.save(genesis_event.clone()).await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::action::global_index_action::GlobalIndexSyncRequestTestAction;
    use crate::meta_tests::fixture::ClientDeviceFixture;
    use crate::meta_tests::spec::global_index_specs::GlobalIndexSpec;

    #[tokio::test]
    async fn test_init() -> anyhow::Result<()> {
        let gi_request_action = GlobalIndexSyncRequestTestAction::init().await?;
        let device_fixture = ClientDeviceFixture::default();

        let _ = gi_request_action.send_request(device_fixture.device_creds.device).await;

        let gi_spec = GlobalIndexSpec {
            repo: gi_request_action.server_node.p_obj.repo.clone(),
            server_device: gi_request_action.server_node.device.device.clone(),
        };

        gi_spec.check().await?;

        Ok(())
    }
}
