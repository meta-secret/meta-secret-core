use std::error::Error;

use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::models::{KvLogEventUpdate};
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::models::{
    GenericKvLogEvent, KeyIdGen, KvKeyId, KvLogEvent, LogEventKeyBasedRecord, ObjectCreator, ObjectDescriptor,
    PublicKeyRecord,
};
use std::rc::Rc;
use async_trait::async_trait;
use std::marker::PhantomData;

pub struct PersistentObject<Repo: KvLogEventRepo<Err>, Err: Error> {
    pub repo: Rc<Repo>,
    pub global_index: PersistentGlobalIndex<Repo, Err>,
}

impl<Repo: KvLogEventRepo<Err>, Err: Error> PersistentObject<Repo, Err> {

    pub async fn get_object_events_from_beginning(
        &self,
        obj_desc: &ObjectDescriptor,
        server_pk: &PublicKeyRecord,
    ) -> Result<Vec<GenericKvLogEvent>, Err> {
        let formation_id = KvKeyId::formation(obj_desc);
        let mut commit_log = self.find_object_events(&formation_id.obj_id()).await;

        //check if genesis event exists for vaults index
        if commit_log.is_empty() {
            let formation_event = self.global_index.init(server_pk).await?;
            let genesis_event = GenericKvLogEvent::Update(KvLogEventUpdate::Genesis { event: formation_event });
            commit_log.push(genesis_event);
        }

        Ok(commit_log)
    }

    pub async fn find_object_events(&self, tail_id: &ObjectId) -> Vec<GenericKvLogEvent> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        let mut curr_tail_id = tail_id.clone();
        loop {
            let curr_db_event_result = self.repo.find_one(&curr_tail_id).await;

            match curr_db_event_result {
                Ok(maybe_curr_db_event) => match maybe_curr_db_event {
                    Some(curr_db_event) => {
                        curr_tail_id = curr_db_event.key().key_id.next().obj_id();
                        commit_log.push(curr_db_event);
                    }
                    None => {
                        break;
                    }
                },
                Err(_) => {
                    break;
                }
            }
        }

        commit_log
    }

    pub async fn acuire_next_free_id(&self, obj_desc: &ObjectDescriptor) -> KvKeyId {
        let formation_id = KvKeyId::formation(obj_desc);

        let mut existing_id = formation_id.clone();
        let mut curr_tail_id = formation_id;
        loop {
            let global_idx_result = self
                .repo
                .find_one(&curr_tail_id.obj_id())
                .await;

            match global_idx_result {
                Ok(maybe_idx) => match maybe_idx {
                    Some(idx) => {
                        existing_id = idx.key().key_id.clone();
                        curr_tail_id = existing_id.next();
                    }
                    None => {
                        break;
                    }
                },
                Err(_) => {
                    break;
                }
            }
        }

        existing_id
    }
}

#[async_trait(? Send)]
pub trait PersistentGlobalIndexApi<Repo: KvLogEventRepo<Err>, Err: Error> {
    async fn init(&self, public_key: &PublicKeyRecord) -> Result<KvLogEvent<PublicKeyRecord>, Err>;
}

pub struct PersistentGlobalIndex<Repo:  KvLogEventRepo<Err>, Err: Error> {
    pub repo: Rc<Repo>,
    pub _phantom: PhantomData<Err>,
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo<Err>, Err: Error> PersistentGlobalIndexApi<Repo, Err> for PersistentGlobalIndex<Repo, Err> {
    ///create a genesis event and save into the database
    async fn init(&self, public_key: &PublicKeyRecord) -> Result<KvLogEvent<PublicKeyRecord>, Err> {
        let formation_log_event = KvLogEvent::global_index_formation(public_key);
        let formation_event = GenericKvLogEvent::Update(KvLogEventUpdate::Genesis {
            event: formation_log_event.clone(),
        });

        self.repo.save_event(&formation_event).await?;

        Ok(formation_log_event)
    }
}