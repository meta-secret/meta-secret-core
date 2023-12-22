use std::sync::Arc;
use tracing_attributes::instrument;
use tracing::info;
use crate::node::common::model::device::DeviceData;
use crate::node::db::descriptors::global_index::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::{KvLogEventRepo};

pub struct PersistentGlobalIndex<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentGlobalIndex<Repo> {

    ///create a genesis event and save into the database
    #[instrument(skip(self))]
    pub async fn init(&self, device: DeviceData) -> anyhow::Result<()> {
        //Check if all required persistent objects has been created
        let gi_obj_desc = ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index);

        let maybe_unit_event = {
            let gi_unit = ObjectId::unit(gi_obj_desc.clone());
            self.p_obj.repo.find_one(gi_unit).await
        };

        let maybe_genesis_event = self.p_obj.repo
            .find_one(ObjectId::genesis(gi_obj_desc))
            .await;

        let gi_genesis_exists = matches!(maybe_unit_event, Ok(Some(_)));
        let gi_unit_exists = matches!(maybe_genesis_event, Ok(Some(_)));

        //If either of unit or genesis not exists then create initial records for the global index
        if gi_unit_exists && gi_genesis_exists {
            return Ok(());
        }

        info!("Init global index");

        let unit_event = GlobalIndexObject::Unit(KvLogEvent::global_index_unit())
            .to_generic();
        let genesis_event = GlobalIndexObject::Genesis(KvLogEvent::global_index_genesis(device))
            .to_generic();

        self.p_obj.repo.save(unit_event.clone()).await?;
        self.p_obj.repo.save(genesis_event.clone()).await?;


        Ok(())
    }
}
