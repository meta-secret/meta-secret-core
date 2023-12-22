use std::sync::Arc;
use tracing_attributes::instrument;
use tracing::info;
use crate::node::common::model::device::DeviceData;
use crate::node::db::descriptors::global_index::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::{KvLogEventRepo};

pub struct PersistentGlobalIndex<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub server_device: DeviceData
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
        let genesis_event = GlobalIndexObject::Genesis(KvLogEvent::global_index_genesis(self.server_device.clone()))
            .to_generic();

        self.p_obj.repo.save(unit_event.clone()).await?;
        self.p_obj.repo.save(genesis_event.clone()).await?;


        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use crate::crypto::keys::{KeyManager, OpenBox};
    use crate::node::common::model::device::{DeviceData, DeviceName};
    use crate::node::db::descriptors::global_index::GlobalIndexDescriptor;
    use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
    use crate::node::db::events::generic_log_event::ObjIdExtractor;
    use crate::node::db::events::global_index::GlobalIndexObject;
    use crate::node::db::events::object_id::ObjectId;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::global_index::PersistentGlobalIndex;
    use crate::node::db::objects::persistent_object::PersistentObject;

    #[tokio::test]
    async fn test_init() -> anyhow::Result<()> {
        let repo = Arc::new(InMemKvLogEventRepo::default());

        let server_device = {
            let secret_box = KeyManager::generate_secret_box();
            let open_box = OpenBox::from(&secret_box);
            DeviceData::from(DeviceName::from("test_device"), open_box)
        };

        let p_global_index = {
            let p_obj = Arc::new(PersistentObject::new(repo.clone()));
            PersistentGlobalIndex { p_obj, server_device }
        };

        p_global_index.init().await?;

        let db = repo.get_db().await;
        assert_eq!(db.len(), 2);

        let gi_obj_desc = GlobalIndexDescriptor::Index.to_obj_desc();

        let unit_event = {
            let unit_id = ObjectId::unit(gi_obj_desc.clone());
            let event = db.get(&unit_id).unwrap();
            GlobalIndexObject::try_from(event.clone())?
        };
        assert_eq!(unit_event.obj_id().get_unit_id().id.id, 0);

        let genesis_event = {
            let genesis_id = ObjectId::genesis(gi_obj_desc.clone());
            let event = db.get(&genesis_id).unwrap();
            GlobalIndexObject::try_from(event.clone())?
        };

        if let GlobalIndexObject::Genesis(log_event) = genesis_event {
            assert_eq!(log_event.value, server_device);
        } else {
            panic!("Invalid Genesis event");
        }

        Ok(())
    }
}