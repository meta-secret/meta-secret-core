use std::collections::HashMap;
use std::error::Error;

use crate::node::db::generic_db::{FindOneQuery, KvLogEventRepo, SaveCommand};
use crate::node::db::models::{GlobalIndexObject};
use crate::node::db::events::object_id::{ObjectId, IdGen};
use crate::node::db::models::{
    GenericKvLogEvent, KvLogEvent, LogEventKeyBasedRecord, ObjectCreator, ObjectDescriptor,
    PublicKeyRecord,
};
use std::rc::Rc;
use async_trait::async_trait;
use std::marker::PhantomData;
use std::cell::RefCell;

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
        let formation_id = ObjectId::unit(obj_desc);
        let mut commit_log = self.find_object_events(&formation_id).await;

        //check if genesis event exists for vaults index
        if commit_log.is_empty() {
            let gi_events = self.global_index.init(server_pk).await?;
            commit_log.extend(gi_events);
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
                        curr_tail_id = curr_db_event.key().obj_id.next();
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

    pub async fn find_tail_id(&self, curr_id: &ObjectId) -> ObjectId {

        let mut existing_id = curr_id.clone();
        let mut curr_tail_id = curr_id.clone();
        loop {
            let global_idx_result = self
                .repo
                .find_one(&curr_tail_id)
                .await;

            match global_idx_result {
                Ok(maybe_idx) => match maybe_idx {
                    Some(idx) => {
                        existing_id = idx.key().obj_id.clone();
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

    pub async fn find_tail_id_by_obj_desc(&self, obj_desc: &ObjectDescriptor) -> ObjectId {
        let formation_id = ObjectId::unit(obj_desc);
        self.find_tail_id(&formation_id).await
    }
}

#[async_trait(? Send)]
pub trait PersistentGlobalIndexApi<Repo: KvLogEventRepo<Err>, Err: Error> {
    async fn init(&self, public_key: &PublicKeyRecord) -> Result<Vec<GenericKvLogEvent>, Err>;
}

pub struct PersistentGlobalIndex<Repo:  KvLogEventRepo<Err>, Err: Error> {
    pub repo: Rc<Repo>,
    pub _phantom: PhantomData<Err>,
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo<Err>, Err: Error> PersistentGlobalIndexApi<Repo, Err> for PersistentGlobalIndex<Repo, Err> {
    ///create a genesis event and save into the database
    async fn init(&self, public_key: &PublicKeyRecord) -> Result<Vec<GenericKvLogEvent>, Err> {
        let unit_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Unit {
            event: KvLogEvent::global_index_unit()
        });

        self.repo
            .save_event(&unit_event)
            .await?;

        let genesis_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Genesis {
            event: KvLogEvent::global_index_genesis(public_key),
        });

        self.repo
            .save_event(&genesis_event)
            .await?;

        Ok(vec![unit_event, genesis_event])
    }
}

pub struct InMemKvLogEventRepo {
    pub db: RefCell<HashMap<ObjectId, GenericKvLogEvent>>
}

impl InMemKvLogEventRepo {
    fn new() -> InMemKvLogEventRepo {
        InMemKvLogEventRepo {
            db:  RefCell::new(HashMap::new()),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum InMemDbError {

}

#[async_trait(? Send)]
impl FindOneQuery<InMemDbError> for InMemKvLogEventRepo {
    async fn find_one(&self, key: &ObjectId) -> Result<Option<GenericKvLogEvent>, InMemDbError> {
        let maybe_value = self.db.borrow().get(key).cloned();
        Ok(maybe_value)
    }
}

#[async_trait(? Send)]
impl SaveCommand<InMemDbError> for InMemKvLogEventRepo {
    async fn save(&self, key: &ObjectId, value: &GenericKvLogEvent) -> Result<(), InMemDbError> {
        self.db.borrow_mut().insert(key.clone(), value.clone());
        Ok(())
    }
}

impl KvLogEventRepo<InMemDbError> for InMemKvLogEventRepo {

}

#[cfg(test)]
mod test {
    use std::marker::PhantomData;
    use std::ops::Deref;
    use std::rc::Rc;
    use crate::node::db::generic_db::SaveCommand;
    use crate::node::db::models::{GenericKvLogEvent, GlobalIndexObject, KvLogEvent, ObjectDescriptor, PublicKeyRecord};
    use crate::node::server::persistent_object::{PersistentObject, PersistentGlobalIndex, InMemKvLogEventRepo};
    use crate::crypto::keys::KeyManager;
    use crate::models::DeviceInfo;

    #[tokio::test]
    async fn test() {
        let repo = InMemKvLogEventRepo::new();
        let repo_rc = Rc::new(repo);

        let persistent_object = PersistentObject {
            repo: repo_rc.clone(),
            global_index: PersistentGlobalIndex {
                repo: repo_rc.clone(),
                _phantom: PhantomData,
            },
        };

        let s_box = KeyManager::generate_security_box("test_vault".to_string());
        let device = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let user_sig = s_box.get_user_sig(&device);

        let server_pk = PublicKeyRecord::from(user_sig.public_key.as_ref().clone());
        let genesis_log_event = KvLogEvent::global_index_genesis(&server_pk);
        let genesis_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Genesis {event: genesis_log_event });

        repo_rc.save_event(&genesis_event).await.unwrap();

        let index_desc = ObjectDescriptor::GlobalIndex;
        let free_id = persistent_object.find_tail_id_by_obj_desc(&index_desc).await;

        println!("Db: ");
        for (_id, event) in repo_rc.db.borrow().deref() {
            println!(" {:?}", event);
        }
        println!("tail id: {:?}", free_id);
    }
}