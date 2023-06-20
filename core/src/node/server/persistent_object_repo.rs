use std::error::Error;

use async_trait::async_trait;

use crate::node::db::generic_db::{FindOneQuery, KvLogEventRepo};
use crate::node::db::models::{Descriptors, KvLogEventUpdate};
use crate::node::db::models::{
    GenericKvLogEvent, KeyIdGen, KvKey, KvKeyId, KvLogEvent, LogEventKeyBasedRecord, ObjectCreator, ObjectDescriptor,
    PublicKeyRecord,
};

pub trait ObjectFormation {
    fn formation_event(&self, obj_desc: &ObjectDescriptor, server_pk: &PublicKeyRecord) -> KvLogEvent<PublicKeyRecord> {
        KvLogEvent {
            key: KvKey::formation(obj_desc),
            value: server_pk.clone(),
        }
    }
}

#[async_trait(? Send)]
pub trait PersistentObjectRepo<Err> {
    async fn find_object_events(&self, tail_id: &str) -> Vec<GenericKvLogEvent>;

    async fn get_object_events_from_beginning(
        &self,
        obj_desc: &ObjectDescriptor,
        server_pk: &PublicKeyRecord,
    ) -> Vec<GenericKvLogEvent>;
}

#[async_trait(? Send)]
impl<T, Err> PersistentObjectRepo<Err> for T
where
    T: PersistentObjectQueries<Err> + PersistentObjectCommands<Err>,
{
    async fn get_object_events_from_beginning(
        &self,
        obj_desc: &ObjectDescriptor,
        server_pk: &PublicKeyRecord,
    ) -> Vec<GenericKvLogEvent> {
        let formation_id = KvKeyId::formation(obj_desc);
        let mut commit_log = self.find_object_events(formation_id.obj_id.id.as_str()).await;

        //check if genesis event exists for vaults index
        if commit_log.is_empty() {
            let formation_event = self.init_global_index(server_pk).await;
            let genesis_event = GenericKvLogEvent::Update(KvLogEventUpdate::Genesis { event: formation_event });
            commit_log.push(genesis_event);
        }

        commit_log
    }

    async fn find_object_events(&self, tail_id: &str) -> Vec<GenericKvLogEvent> {
        self.find_object_events_internal(tail_id).await
    }
}

#[async_trait(? Send)]
pub trait PersistentObjectQueries<Err> {
    async fn find_object_events_internal(&self, tail_id: &str) -> Vec<GenericKvLogEvent>;
    async fn get_next_free_id(&self, obj_desc: &ObjectDescriptor) -> KvKeyId;
}

#[async_trait(? Send)]
pub trait PersistentObjectCommands<Err> {
    async fn init_global_index(&self, public_key: &PublicKeyRecord) -> KvLogEvent<PublicKeyRecord>;
}

#[async_trait(? Send)]
impl<T, Err> PersistentObjectQueries<Err> for T
where
    T: FindOneQuery<Err>,
    Err: Error,
{
    async fn get_next_free_id(&self, obj_desc: &ObjectDescriptor) -> KvKeyId {
        let formation_id = KvKeyId::formation(obj_desc);

        let mut existing_id = formation_id.clone();
        let mut curr_tail_id = formation_id;
        loop {
            let global_idx_result = self.find_one(curr_tail_id.obj_id.id.as_str()).await;

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

    async fn find_object_events_internal(&self, tail_id: &str) -> Vec<GenericKvLogEvent> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        let mut curr_tail_id = tail_id.to_string();
        loop {
            let curr_db_event_result = self.find_one(curr_tail_id.as_str()).await;

            match curr_db_event_result {
                Ok(maybe_curr_db_event) => match maybe_curr_db_event {
                    Some(curr_db_event) => {
                        curr_tail_id = curr_db_event.key().key_id.next().obj_id.id;
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
}

#[async_trait(? Send)]
impl<T, Err> PersistentObjectCommands<Err> for T
where
    T: KvLogEventRepo<Err> + ObjectFormation,
    Err: Error,
{
    async fn init_global_index(&self, public_key: &PublicKeyRecord) -> KvLogEvent<PublicKeyRecord> {
        //create a genesis event and save into the database
        let formation_log_event = self.formation_event(&Descriptors::global_index(), public_key);
        let formation_event = GenericKvLogEvent::Update(KvLogEventUpdate::Genesis {
            event: formation_log_event.clone(),
        });

        self.save(&formation_event).await.unwrap();

        formation_log_event
    }
}
