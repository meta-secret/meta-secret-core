use std::sync::Arc;

use crate::node::{
    common::model::device::DeviceData,
    db::{
        descriptors::{global_index::GlobalIndexDescriptor, object_descriptor::ToObjectDescriptor},
        events::{generic_log_event::ObjIdExtractor, global_index::GlobalIndexObject, object_id::ObjectId},
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::Result;

pub struct GlobalIndexSpec<Repo: KvLogEventRepo> {
    pub repo: Arc<Repo>,
    pub server_device: DeviceData,
}

impl<Repo: KvLogEventRepo> GlobalIndexSpec<Repo> {
    pub async fn check(&self) -> Result<()> {
        let gi_obj_desc = GlobalIndexDescriptor::Index.to_obj_desc();

        let unit_event = {
            let unit_id = ObjectId::unit(gi_obj_desc.clone());
            let event = self.repo.find_one(unit_id).await?.unwrap();
            GlobalIndexObject::try_from(event)?
        };
        assert_eq!(unit_event.obj_id().get_unit_id().id.id, 0);

        let genesis_event = {
            let genesis_id = ObjectId::genesis(gi_obj_desc.clone());
            let event = self.repo.find_one(genesis_id).await?.unwrap();
            GlobalIndexObject::try_from(event.clone())?
        };

        if let GlobalIndexObject::Genesis(log_event) = genesis_event {
            assert_eq!(log_event.value, self.server_device);
        } else {
            panic!("Invalid Genesis event");
        }

        Ok(())
    }
}
