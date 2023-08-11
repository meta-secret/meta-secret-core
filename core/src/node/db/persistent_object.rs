use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::marker::PhantomData;
use std::rc::Rc;

use async_trait::async_trait;

use crate::node::db::events::object_id::{IdGen, ObjectId};
use crate::node::db::generic_db::{FindOneQuery, KvLogEventRepo, SaveCommand};
use crate::node::db::models::{DbTail, DbTailObject, GlobalIndexObject, KvKey, KvLogEventLocal};
use crate::node::db::models::{
    GenericKvLogEvent, KvLogEvent, LogEventKeyBasedRecord, ObjectCreator, ObjectDescriptor, PublicKeyRecord,
};
use crate::node::server::data_sync::MetaLogger;

pub struct PersistentObject {
    pub repo: Rc<dyn KvLogEventRepo>,
    pub global_index: PersistentGlobalIndex,
    pub logger: Rc<dyn MetaLogger>,
}

impl PersistentObject {
    pub async fn get_object_events_from_beginning(
        &self,
        obj_desc: &ObjectDescriptor,
        server_pk: &PublicKeyRecord,
    ) -> Result<Vec<GenericKvLogEvent>, Box<dyn Error>> {
        self.logger.log("get_object_events_from_beginning");

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
        //logger.log("find_object_events");

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

    pub async fn find_tail_id(&self, curr_id: &ObjectId) -> Option<ObjectId> {
        let initial_global_idx_result = self.repo.find_one(curr_id).await;

        match initial_global_idx_result {
            Ok(maybe_gi) => match maybe_gi {
                None => None,
                Some(gi) => {
                    let mut existing_id = gi.key().obj_id.clone();
                    let mut curr_tail_id = gi.key().obj_id.clone();

                    loop {
                        let global_idx_result = self.repo.find_one(&curr_tail_id).await;

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

                    Some(existing_id)
                }
            },
            Err(_) => None,
        }
    }

    pub async fn find_tail_id_by_obj_desc(&self, obj_desc: &ObjectDescriptor) -> Option<ObjectId> {
        let unit_id = ObjectId::unit(obj_desc);
        self.find_tail_id(&unit_id).await
    }

    pub async fn get_db_tail(&self, vault_name: &str) -> Result<DbTail, Box<dyn Error>> {
        let obj_id = ObjectId::unit(&ObjectDescriptor::DbTail);
        let maybe_db_tail = self.repo.find_one(&obj_id).await?;

        match maybe_db_tail {
            None => {
                let db_tail = DbTail {
                    vault_id: DbTailObject::Empty {
                        unit_id: ObjectId::vault_unit(vault_name),
                    },
                    maybe_global_index_id: None,
                    maybe_mem_pool_id: None,
                    meta_pass_id: DbTailObject::Empty {
                        unit_id: ObjectId::meta_pass_unit(vault_name),
                    },
                };

                let tail_event = {
                    let event = KvLogEvent {
                        key: KvKey::unit(&ObjectDescriptor::DbTail),
                        value: db_tail.clone(),
                    };
                    GenericKvLogEvent::LocalEvent(KvLogEventLocal::DbTail { event: Box::new(event) })
                };

                self.repo.save_event(&tail_event).await?;
                Ok(db_tail)
            }
            Some(db_tail) => match db_tail {
                GenericKvLogEvent::LocalEvent(local_evt) => match local_evt {
                    KvLogEventLocal::DbTail { event } => Ok(event.value),
                    _ => {
                        panic!("DbTail. Invalid data");
                    }
                },
                _ => {
                    panic!("DbTail. Invalid event type");
                }
            },
        }
    }
}

#[async_trait(? Send)]
pub trait PersistentGlobalIndexApi {
    async fn init(&self, public_key: &PublicKeyRecord) -> Result<Vec<GenericKvLogEvent>, Box<dyn Error>>;
}

pub struct PersistentGlobalIndex {
    pub repo: Rc<dyn KvLogEventRepo>,
    pub _phantom: PhantomData<Box<dyn Error>>,
    pub logger: Rc<dyn MetaLogger>,
}

#[async_trait(? Send)]
impl PersistentGlobalIndexApi for PersistentGlobalIndex {
    ///create a genesis event and save into the database
    async fn init(&self, public_key: &PublicKeyRecord) -> Result<Vec<GenericKvLogEvent>, Box<dyn Error>> {
        self.logger.log("Init global index");

        let unit_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Unit {
            event: KvLogEvent::global_index_unit(),
        });

        self.repo.save_event(&unit_event).await?;

        let genesis_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Genesis {
            event: KvLogEvent::global_index_genesis(public_key),
        });

        self.repo.save_event(&genesis_event).await?;

        Ok(vec![unit_event, genesis_event])
    }
}

pub struct InMemKvLogEventRepo {
    pub db: RefCell<HashMap<ObjectId, GenericKvLogEvent>>,
}

impl InMemKvLogEventRepo {
    fn new() -> InMemKvLogEventRepo {
        InMemKvLogEventRepo {
            db: RefCell::new(HashMap::new()),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum InMemDbError {}

#[async_trait(? Send)]
impl FindOneQuery for InMemKvLogEventRepo {
    async fn find_one(&self, key: &ObjectId) -> Result<Option<GenericKvLogEvent>, Box<dyn Error>> {
        let maybe_value = self.db.borrow().get(key).cloned();
        Ok(maybe_value)
    }
}

#[async_trait(? Send)]
impl SaveCommand for InMemKvLogEventRepo {
    async fn save(&self, key: &ObjectId, value: &GenericKvLogEvent) -> Result<(), Box<dyn Error>> {
        self.db.borrow_mut().insert(key.clone(), value.clone());
        Ok(())
    }
}

impl KvLogEventRepo for InMemKvLogEventRepo {}

impl PersistentObject {
    pub fn new(repo: Rc<dyn KvLogEventRepo>, logger: Rc<dyn MetaLogger>) -> Self {
        PersistentObject {
            repo: repo.clone(),
            global_index: PersistentGlobalIndex {
                repo,
                _phantom: PhantomData,
                logger: logger.clone(),
            },
            logger,
        }
    }
}

#[cfg(test)]
mod test {
    use std::ops::Deref;
    use std::rc::Rc;

    use crate::crypto::keys::KeyManager;
    use crate::models::DeviceInfo;
    use crate::node::db::generic_db::SaveCommand;
    use crate::node::db::models::{
        GenericKvLogEvent, GlobalIndexObject, KvLogEvent, ObjectDescriptor, PublicKeyRecord,
    };
    use crate::node::db::persistent_object::{InMemKvLogEventRepo, PersistentObject};
    use crate::node::server::data_sync::DefaultMetaLogger;

    #[tokio::test]
    async fn test() {
        let repo = InMemKvLogEventRepo::new();
        let repo_rc = Rc::new(repo);

        let persistent_object = PersistentObject::new(repo_rc.clone(), Rc::new(DefaultMetaLogger {}));

        let s_box = KeyManager::generate_security_box("test_vault".to_string());
        let device = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let user_sig = s_box.get_user_sig(&device);

        let server_pk = PublicKeyRecord::from(user_sig.public_key.as_ref().clone());
        let genesis_log_event = KvLogEvent::global_index_genesis(&server_pk);
        let genesis_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Genesis {
            event: genesis_log_event,
        });

        repo_rc.save_event(&genesis_event).await.unwrap();

        let free_id = persistent_object
            .find_tail_id_by_obj_desc(&ObjectDescriptor::GlobalIndex)
            .await;

        println!("Db: ");
        for (_id, event) in repo_rc.db.borrow().deref() {
            println!(" {:?}", event);
        }
        println!("tail id: {:?}", free_id);
    }
}
