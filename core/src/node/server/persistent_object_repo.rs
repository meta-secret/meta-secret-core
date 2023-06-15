use async_trait::async_trait;

use crate::models::Base64EncodedText;
use crate::node::db::generic_db::{FindOneQuery, KvLogEventRepo};
use crate::node::db::models::{
    AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, KvValueType, ObjectCreator, ObjectDescriptor,
};
use crate::node::db::models::Descriptors;

pub trait ObjectFormation {
    fn formation_event(&self, obj_desc: &ObjectDescriptor, server_pk: &Base64EncodedText) -> KvLogEvent {
        KvLogEvent {
            key: KvKey::formation(obj_desc),
            cmd_type: AppOperationType::Update(AppOperation::ObjectFormation),
            val_type: KvValueType::DsaPublicKey,
            value: serde_json::to_value(server_pk).unwrap(),
        }
    }
}

#[async_trait(? Send)]
pub trait PersistentObjectRepo {
    async fn get_object_events_from_beginning(
        &self,
        obj_desc: &ObjectDescriptor,
        server_pk: &Base64EncodedText,
    ) -> Vec<KvLogEvent>;

    async fn find_object_events(&self, tail_id: &str) -> Vec<KvLogEvent>;
}

#[async_trait(? Send)]
impl<T> PersistentObjectRepo for T
    where
        T: PersistentObjectQueries + PersistentObjectCommands,
{
    async fn get_object_events_from_beginning(
        &self,
        obj_desc: &ObjectDescriptor,
        server_pk: &Base64EncodedText,
    ) -> Vec<KvLogEvent> {
        let formation_id = KvKeyId::formation(obj_desc);
        let mut commit_log = self.find_object_events(formation_id.obj_id.id.as_str()).await;

        //check if genesis event exists for vaults index
        if commit_log.is_empty() {
            let formation_event = self.init_global_index(server_pk).await;
            commit_log.push(formation_event);
        }

        commit_log
    }

    async fn find_object_events(&self, tail_id: &str) -> Vec<KvLogEvent> {
        self.find_object_events_internal(tail_id).await
    }
}

#[async_trait(? Send)]
pub trait PersistentObjectQueries {
    async fn find_object_events_internal(&self, tail_id: &str) -> Vec<KvLogEvent>;
    async fn get_next_free_id(&self, obj_desc: &ObjectDescriptor) -> KvKeyId;
}

#[async_trait(? Send)]
pub trait PersistentObjectCommands {
    async fn init_global_index(&self, public_key: &Base64EncodedText) -> KvLogEvent;
}

#[async_trait(? Send)]
impl<T: FindOneQuery> PersistentObjectQueries for T {
    async fn get_next_free_id(&self, obj_desc: &ObjectDescriptor) -> KvKeyId {
        let formation_id = KvKeyId::formation(obj_desc);

        let mut existing_id = formation_id.clone();
        let mut curr_tail_id = formation_id;
        loop {
            let global_idx_result = self.find_one(curr_tail_id.obj_id.id.as_str()).await;

            match global_idx_result {
                Ok(maybe_idx) => match maybe_idx {
                    Some(idx) => {
                        existing_id = idx.key.key_id;
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

    async fn find_object_events_internal(&self, tail_id: &str) -> Vec<KvLogEvent> {
        let mut commit_log: Vec<KvLogEvent> = vec![];

        let mut curr_tail_id = tail_id.to_string();
        loop {
            let curr_db_event_result = self.find_one(curr_tail_id.as_str()).await;

            match curr_db_event_result {
                Ok(maybe_curr_db_event) => match maybe_curr_db_event {
                    Some(curr_db_event) => {
                        curr_tail_id = curr_db_event.key.key_id.next().obj_id.id;
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
impl<T: KvLogEventRepo + ObjectFormation> PersistentObjectCommands for T {
    async fn init_global_index(&self, public_key: &Base64EncodedText) -> KvLogEvent {
        //create a genesis event and save into the database
        let formation_event = self.formation_event(&Descriptors::global_index(), public_key);

        self.save(&formation_event).await.unwrap();

        formation_event
    }
}
