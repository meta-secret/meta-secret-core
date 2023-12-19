use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info, instrument, Instrument};

use crate::node::common::model::device::DeviceData;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::{Next, ObjectId};
use crate::node::db::objects::persistent_object_navigator::PersistentObjectNavigator;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct PersistentObject<Repo: KvLogEventRepo> {
    pub repo: Arc<Repo>,
    pub global_index: Arc<PersistentGlobalIndex<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentObject<Repo> {

    #[instrument(skip_all)]
    pub async fn get_object_events_from_beginning(
        &self,
        obj_desc: ObjectDescriptor,
    ) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        debug!("get_object_events_from_beginning");

        let unit_id = ObjectId::unit(obj_desc);
        let commit_log = self.find_object_events(unit_id).await?;

        Ok(commit_log)
    }

    #[instrument(skip_all)]
    pub async fn find_object_events(&self, tail_id: ObjectId) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        let mut curr_tail_id = tail_id.clone();
        loop {
            let maybe_curr_db_event = self.repo.find_one(curr_tail_id.clone()).await?;

            if let Some(curr_db_event) = maybe_curr_db_event {
                if let GenericKvLogEvent::SharedSecret(_) = &curr_db_event {
                    self.repo.delete(curr_tail_id.clone()).in_current_span().await;
                }

                curr_tail_id = curr_tail_id.next();
                commit_log.push(curr_db_event);
            } else {
                break;
            }
        }

        Ok(commit_log)
    }

    #[instrument(skip_all)]
    pub async fn find_tail_event(&self, obj_desc: ObjectDescriptor) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let maybe_tail_id = self
            .find_tail_id_by_obj_desc(obj_desc)
            .await?;

        self.find_event_by_id(maybe_tail_id).await
    }

    pub async fn find_tail_event_by_obj_id(&self, obj_id: ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let maybe_tail_id = self.find_tail_id(obj_id).await?;
        self.find_event_by_id(maybe_tail_id).await
    }

    async fn find_event_by_id(&self, maybe_tail_id: Option<ObjectId>) -> anyhow::Result<Option<GenericKvLogEvent>> {
        if let Some(tail_id) = maybe_tail_id {
            let maybe_tail_event = self.repo.find_one(tail_id).await?;
            Ok(maybe_tail_event)
        } else {
            Ok(None)
        }
    }

    #[instrument(skip_all)]
    pub async fn find_free_id_by_obj_desc(&self, obj_desc: ObjectDescriptor) -> anyhow::Result<ObjectId> {
        let maybe_tail_id = self
            .find_tail_id_by_obj_desc(obj_desc.clone())
            .await?;

        let free_id = maybe_tail_id
            .map(|tail_id| tail_id.next())
            .unwrap_or(ObjectId::unit(obj_desc));

        Ok(free_id)
    }

    #[instrument(skip_all)]
    pub async fn find_free_id(&self, obj_id: ObjectId) -> anyhow::Result<ObjectId> {
        let maybe_tail_id = self
            .find_tail_id(obj_id.clone())
            .await?;

        let free_id = maybe_tail_id
            .map(|tail_id| tail_id.next())
            .unwrap_or(ObjectId::from(obj_id.get_unit_id()));

        Ok(free_id)
    }

    #[instrument(skip_all)]
    pub async fn find_tail_id_by_obj_desc(&self, obj_desc: ObjectDescriptor) -> anyhow::Result<Option<ObjectId>> {
        let unit_id = ObjectId::unit(obj_desc);
        self.find_tail_id(unit_id).await
    }

    #[instrument(skip_all)]
    pub async fn navigator(&self, obj_id: ObjectId) -> PersistentObjectNavigator<Repo> {
        PersistentObjectNavigator::build(self.repo.clone(), obj_id).await
    }

    #[instrument(skip_all)]
    pub async fn find_tail_id(&self, curr_id: ObjectId) -> anyhow::Result<Option<ObjectId>> {
        let maybe_event = self.repo.find_one(curr_id.clone()).await?;

        if let Some(curr_event) = maybe_event {
            let mut existing_id = curr_event.obj_id();
            let mut curr_tail_id = curr_event.obj_id();

            loop {
                let found_event = self.repo.find_one(curr_tail_id.clone()).await?;

                if let Some(curr_tail) = found_event {
                    let curr_obj_id = curr_tail.obj_id();
                    existing_id = curr_obj_id.clone();
                    curr_tail_id = curr_obj_id.next();
                } else {
                    break;
                }
            }

            Ok(Some(existing_id))
        } else {
            Ok(None)
        }
    }

    #[instrument(skip_all)]
    pub async fn get_db_tail(&self) -> anyhow::Result<DbTail> {
        let maybe_db_tail = {
            let db_tail_unit_id = ObjectId::unit(ObjectDescriptor::DbTail);
            self.repo.find_one(db_tail_unit_id).in_current_span().await?
        };

        if let Some(db_tail_event) = maybe_db_tail {
            db_tail_event.to_db_tail()
        } else {
            let db_tail = DbTail::Basic { global_index_id: None };
            let tail_event = GenericKvLogEvent::db_tail(db_tail.clone());

            self.repo.save(tail_event).await?;
            Ok(db_tail)
        }
    }
}

#[async_trait(? Send)]
pub trait PersistentGlobalIndexApi {
    async fn gi_init(&self, public_key: DeviceData) -> anyhow::Result<Vec<GenericKvLogEvent>>;
}

pub struct PersistentGlobalIndex<Repo: KvLogEventRepo> {
    pub repo: Arc<Repo>,
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo> PersistentGlobalIndexApi for PersistentGlobalIndex<Repo> {

    ///create a genesis event and save into the database
    #[instrument(skip(self))]
    async fn gi_init(&self, public_key: DeviceData) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        info!("Init global index");

        let unit_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Unit(KvLogEvent::global_index_unit()));

        self.repo.save(unit_event.clone()).await?;

        let genesis_event = GlobalIndexObject::Genesis(KvLogEvent::global_index_genesis(public_key))
            .to_generic();

        self.repo.save(genesis_event.clone()).await?;

        Ok(vec![unit_event, genesis_event])
    }
}

impl<Repo: KvLogEventRepo> PersistentObject<Repo> {
    pub fn new(repo: Arc<Repo>) -> Self {
        PersistentObject {
            repo: repo.clone(),
            global_index: Arc::new(PersistentGlobalIndex { repo }),
        }
    }
}
