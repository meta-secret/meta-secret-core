use async_trait::async_trait;

use crate::models::Base64EncodedText;
use crate::node::db::generic_db::{FindOneQuery, SaveCommand};
use crate::node::db::events::global_index;
use crate::node::db::events::global_index::generate_global_index_formation_event;
use crate::node::db::models::{KeyIdGen, KvKeyId, KvLogEvent};

#[async_trait(? Send)]
pub trait PersistentObjectRepo: PersistentObjectQueries + PersistentObjectCommands {
    async fn get_global_index_from_beginning(&self, server_pk: &Base64EncodedText) -> Vec<KvLogEvent> {
        let formation_id = global_index::generate_global_index_formation_key_id();
        let mut commit_log = self.find_object_events(formation_id.key_id.as_str()).await;

        //check if genesis event exists for vaults index
        if commit_log.is_empty() {
            let formation_event = self.init_global_index(server_pk).await;
            commit_log.push(formation_event);
        }

        commit_log
    }
}

//impl<T>

#[async_trait(? Send)]
pub trait PersistentObjectQueries {
    async fn find_object_events(&self, tail_id: &str) -> Vec<KvLogEvent>;

    async fn get_free_global_index_id(&self) -> KvKeyId;
}

#[async_trait(? Send)]
pub trait PersistentObjectCommands {
    async fn init_global_index(&self, public_key: &Base64EncodedText) -> KvLogEvent;
}

#[async_trait(? Send)]
impl<T: FindOneQuery<KvLogEvent>> PersistentObjectQueries for T {
    async fn get_free_global_index_id(&self) -> KvKeyId {
        let formation_id = global_index::generate_global_index_formation_key_id();

        let mut existing_id = formation_id.clone();
        let mut curr_tail_id = formation_id;
        loop {
            let global_idx_result = self.find_one(curr_tail_id.key_id.as_str()).await;

            match global_idx_result {
                Ok(maybe_idx) => {
                    match maybe_idx {
                        Some(_idx) => {
                            existing_id = curr_tail_id.clone();
                            curr_tail_id = curr_tail_id.next();
                        }
                        None => {
                            break;
                        }
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }

        existing_id
    }

    async fn find_object_events(&self, tail_id: &str) -> Vec<KvLogEvent> {
        let mut commit_log: Vec<KvLogEvent> = vec![];

        let mut curr_tail_id = tail_id.to_string();
        loop {
            let curr_db_event_result = self.find_one(curr_tail_id.as_str()).await;

            match curr_db_event_result {
                Ok(maybe_curr_db_event) => {
                    match maybe_curr_db_event {
                        Some(curr_db_event) => {
                            commit_log.push(curr_db_event);
                            curr_tail_id = KvKeyId::generate_next(curr_tail_id.as_str()).key_id.clone();
                        }
                        None => {
                            break;
                        }
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }

        commit_log
    }
}

#[async_trait(? Send)]
impl<S: SaveCommand<KvLogEvent>> PersistentObjectCommands for S {
    async fn init_global_index(&self, public_key: &Base64EncodedText) -> KvLogEvent {
        //create a genesis event and save into the database
        let formation_event = generate_global_index_formation_event(public_key);

        self.save("", &formation_event)
            .await
            .unwrap();

        formation_event
    }
}
