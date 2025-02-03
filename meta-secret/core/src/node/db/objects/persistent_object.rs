use std::sync::Arc;

use tracing::instrument;

use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEventConvertible, ObjIdExtractor};
use crate::node::db::events::object_id::{Next, ObjectId};
use crate::node::db::in_mem_db::InMemKvLogEventRepo;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::Result;

pub struct PersistentObject<Repo: KvLogEventRepo> {
    pub repo: Arc<Repo>,
}

impl<Repo: KvLogEventRepo> PersistentObject<Repo> {
    #[instrument(skip_all)]
    pub async fn get_object_events_from_beginning<ObjDesc: ToObjectDescriptor>(
        &self,
        obj_desc: ObjDesc,
    ) -> Result<Vec<ObjDesc::EventType>> {
        let unit_id = ObjectId::unit_from(obj_desc);
        let commit_log = self.find_object_events(unit_id).await?;

        Ok(commit_log)
    }

    #[instrument(skip_all)]
    pub async fn find_object_events<T: GenericKvLogEventConvertible>(
        &self,
        tail_id: ObjectId,
    ) -> Result<Vec<T>> {
        let mut commit_log: Vec<T> = vec![];

        let mut curr_tail_id = tail_id.clone();
        loop {
            let maybe_curr_db_event = self.repo.find_one_obj(curr_tail_id.clone()).await?;

            if let Some(curr_db_event) = maybe_curr_db_event {
                curr_tail_id = curr_tail_id.next();
                commit_log.push(curr_db_event);
            } else {
                break;
            }
        }

        Ok(commit_log)
    }

    #[instrument(skip_all)]
    pub async fn find_tail_event<Desc: ToObjectDescriptor>(
        &self,
        obj_desc: Desc,
    ) -> Result<Option<Desc::EventType>> {
        let maybe_tail_id = self.find_tail_id_by_obj_desc(obj_desc).await?;
        self.find_event_by_id::<Desc::EventType>(maybe_tail_id)
            .await
    }

    pub async fn find_tail_event_by_obj_id<T: GenericKvLogEventConvertible>(
        &self,
        obj_id: ObjectId,
    ) -> Result<Option<T>> {
        let maybe_tail_id = self.find_tail_id(obj_id).await?;
        self.find_event_by_id(maybe_tail_id).await
    }

    async fn find_event_by_id<T: GenericKvLogEventConvertible>(
        &self,
        maybe_tail_id: Option<ObjectId>,
    ) -> Result<Option<T>> {
        if let Some(tail_id) = maybe_tail_id {
            let maybe_tail_event = self.repo.find_one_obj(tail_id).await?;
            Ok(maybe_tail_event)
        } else {
            Ok(None)
        }
    }

    #[instrument(skip_all)]
    pub async fn find_free_id_by_obj_desc<Desc: ToObjectDescriptor>(
        &self,
        obj_desc: Desc,
    ) -> Result<ObjectId> {
        let maybe_tail_id = self.find_tail_id_by_obj_desc(obj_desc.clone()).await?;

        let free_id = maybe_tail_id
            .map(|tail_id| tail_id.next())
            .unwrap_or(ObjectId::unit_from(obj_desc));

        Ok(free_id)
    }

    #[instrument(skip_all)]
    pub async fn find_free_id(&self, obj_id: ObjectId) -> Result<ObjectId> {
        let maybe_tail_id = self.find_tail_id(obj_id.clone()).await?;

        let free_id = maybe_tail_id
            .map(|tail_id| tail_id.next())
            .unwrap_or(ObjectId::from(obj_id.get_unit_id().clone()));

        Ok(free_id)
    }

    #[instrument(skip_all)]
    pub async fn find_tail_id_by_obj_desc<Desc: ToObjectDescriptor>(
        &self,
        obj_desc: Desc,
    ) -> Result<Option<ObjectId>> {
        let unit_id = ObjectId::unit_from(obj_desc);
        self.find_tail_id(unit_id).await
    }

    #[instrument(skip_all)]
    pub async fn find_tail_id(&self, curr_id: ObjectId) -> Result<Option<ObjectId>> {
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
}

impl<Repo: KvLogEventRepo> PersistentObject<Repo> {
    pub fn new(repo: Arc<Repo>) -> Self {
        PersistentObject { repo }
    }
}

impl PersistentObject<InMemKvLogEventRepo> {
    pub fn in_mem() -> PersistentObject<InMemKvLogEventRepo> {
        let repo = Arc::new(InMemKvLogEventRepo::default());
        PersistentObject::new(repo)
    }
}

#[cfg(test)]
pub mod fixture {
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use std::sync::Arc;

    pub struct PersistentObjectFixture {
        pub client: Arc<PersistentObject<InMemKvLogEventRepo>>,
        pub client_b: Arc<PersistentObject<InMemKvLogEventRepo>>,
        pub vd: Arc<PersistentObject<InMemKvLogEventRepo>>,
        pub server: Arc<PersistentObject<InMemKvLogEventRepo>>,
    }

    impl PersistentObjectFixture {
        pub fn generate() -> Self {
            Self {
                client: Arc::new(PersistentObject::in_mem()),
                client_b: Arc::new(PersistentObject::in_mem()),
                vd: Arc::new(PersistentObject::in_mem()),
                server: Arc::new(PersistentObject::in_mem()),
            }
        }
    }
}
