use std::sync::Arc;

use async_trait::async_trait;
use tracing::info;

use crate::models::user_signature::UserSignature;
use crate::node::db::events::common::{ObjectCreator, PublicKeyRecord};
use crate::node::db::events::db_tail::{DbTail, DbTailObject};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::KvLogEventLocal;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, ObjectId};
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::KvLogEventRepo;

pub struct PersistentObject<Repo: KvLogEventRepo> {
    pub repo: Arc<Repo>,
    pub global_index: Arc<PersistentGlobalIndex<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentObject<Repo> {
    pub async fn get_object_events_from_beginning(
        &self,
        obj_desc: &ObjectDescriptor,
    ) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        info!("get_object_events_from_beginning");

        let unit_id = ObjectId::unit(obj_desc);
        let commit_log = self.find_object_events(&unit_id).await;

        Ok(commit_log)
    }

    pub async fn find_object_events(&self, tail_id: &ObjectId) -> Vec<GenericKvLogEvent> {
        //logger.log("find_object_events");

        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        let mut curr_tail_id = tail_id.clone();
        loop {
            let curr_db_event_result = self.repo.find_one(curr_tail_id.clone()).await;

            match curr_db_event_result {
                Ok(maybe_curr_db_event) => match maybe_curr_db_event {
                    Some(curr_db_event) => {
                        if let GenericKvLogEvent::SharedSecret(_) = &curr_db_event {
                            self.repo.delete(curr_tail_id.clone()).await;
                        }

                        curr_tail_id = curr_tail_id.next();
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

    pub async fn find_tail_id_by_obj_desc(&self, obj_desc: &ObjectDescriptor) -> Option<ObjectId> {
        let unit_id = ObjectId::unit(obj_desc);
        self.find_tail_id(unit_id).await
    }

    pub async fn find_tail_id(&self, curr_id: ObjectId) -> Option<ObjectId> {
        let curr_result = self.repo.find_one(curr_id.clone()).await;

        match curr_result {
            Ok(maybe_event) => match maybe_event {
                None => None,
                Some(_curr_event) => {
                    let mut existing_id = curr_id.clone();
                    let mut curr_tail_id = curr_id.clone();

                    loop {
                        let found_event_result = self.repo.find_one(curr_tail_id.clone()).await;

                        match found_event_result {
                            Ok(Some(_curr_tail)) => {
                                existing_id = curr_tail_id.clone();
                                curr_tail_id = curr_tail_id.next();
                            }
                            _ => break,
                        }
                    }

                    Some(existing_id)
                }
            },
            Err(_) => None,
        }
    }

    pub async fn get_db_tail(&self, vault_name: &str) -> anyhow::Result<DbTail> {
        let obj_id = ObjectId::unit(&ObjectDescriptor::DbTail);
        let maybe_db_tail = self.repo.find_one(obj_id).await?;

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

                self.repo.save_event(tail_event).await?;
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

    pub async fn get_user_sig(&self, tail_id: ObjectId) -> Vec<UserSignature> {
        let sig_result = self.get_vault_unit_signature(tail_id).await;
        match sig_result {
            Ok(Some(vault_sig)) => {
                vec![vault_sig]
            }
            _ => {
                vec![]
            }
        }
    }

    async fn get_vault_unit_signature(&self, tail_id: ObjectId) -> anyhow::Result<Option<UserSignature>> {
        let maybe_unit_event = self.repo.find_one(tail_id).await?;

        match maybe_unit_event {
            Some(GenericKvLogEvent::Vault(VaultObject::Unit { event })) => Ok(Some(event.value)),
            _ => Ok(None),
        }
    }
}

#[async_trait(? Send)]
pub trait PersistentGlobalIndexApi {
    async fn gi_init(&self, public_key: &PublicKeyRecord) -> anyhow::Result<Vec<GenericKvLogEvent>>;
}

pub struct PersistentGlobalIndex<Repo: KvLogEventRepo> {
    pub repo: Arc<Repo>,
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo> PersistentGlobalIndexApi for PersistentGlobalIndex<Repo> {
    ///create a genesis event and save into the database
    async fn gi_init(&self, public_key: &PublicKeyRecord) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        info!("Init global index");

        let unit_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Unit {
            event: KvLogEvent::global_index_unit(),
        });

        self.repo.save_event(unit_event.clone()).await?;

        let genesis_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Genesis {
            event: KvLogEvent::global_index_genesis(public_key),
        });

        self.repo.save_event(genesis_event.clone()).await?;

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

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::crypto::keys::KeyManager;
    use crate::models::DeviceInfo;
    use crate::node::db::events::common::{LogEventKeyBasedRecord, PublicKeyRecord};
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::node::db::events::global_index::{GlobalIndexObject, GlobalIndexRecord};
    use crate::node::db::events::kv_log_event::KvKey;
    use crate::node::db::events::kv_log_event::KvLogEvent;
    use crate::node::db::events::object_descriptor::ObjectDescriptor;
    use crate::node::db::events::object_id::{IdGen, ObjectId};
    use crate::node::db::generic_db::SaveCommand;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;

    #[tokio::test]
    async fn test_find_events() {
        let persistent_object = {
            let repo = Arc::new(InMemKvLogEventRepo::default());
            PersistentObject::new(repo.clone())
        };

        let user_sig = {
            let s_box = KeyManager::generate_security_box("test_vault".to_string());
            let device = DeviceInfo {
                device_id: "a".to_string(),
                device_name: "a".to_string(),
            };
            s_box.get_user_sig(&device)
        };

        let unit_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::unit());
        persistent_object.repo.save_event(unit_event).await.unwrap();

        let genesis_event = {
            let server_pk = PublicKeyRecord::from(user_sig.public_key.as_ref().clone());
            GenericKvLogEvent::GlobalIndex(GlobalIndexObject::genesis(&server_pk))
        };

        let vault_1_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::Update {
            event: KvLogEvent {
                key: KvKey::Empty {
                    obj_desc: ObjectDescriptor::GlobalIndex,
                },
                value: GlobalIndexRecord {
                    vault_id: String::from("vault_1"),
                },
            },
        });

        persistent_object.repo.save_event(genesis_event).await.unwrap();

        persistent_object
            .repo
            .save(ObjectId::global_index_genesis().next(), vault_1_event)
            .await
            .unwrap();

        let tail_id = persistent_object
            .find_tail_id_by_obj_desc(&ObjectDescriptor::GlobalIndex)
            .await
            .unwrap();

        assert_eq!(String::from("GlobalIndex:index::2"), tail_id.id_str());
    }

    #[tokio::test]
    async fn test_global_index() {
        let persistent_object = {
            let repo = Arc::new(InMemKvLogEventRepo::default());
            PersistentObject::new(repo.clone())
        };

        let user_sig = {
            let s_box = KeyManager::generate_security_box("test_vault".to_string());
            let device = DeviceInfo {
                device_id: "a".to_string(),
                device_name: "a".to_string(),
            };
            s_box.get_user_sig(&device)
        };

        let unit_event = GenericKvLogEvent::GlobalIndex(GlobalIndexObject::unit());
        persistent_object.repo.save_event(unit_event).await.unwrap();

        let genesis_event = {
            let server_pk = PublicKeyRecord::from(user_sig.public_key.as_ref().clone());
            GenericKvLogEvent::GlobalIndex(GlobalIndexObject::genesis(&server_pk))
        };

        persistent_object.repo.save_event(genesis_event.clone()).await.unwrap();

        let tail_id = persistent_object
            .find_tail_id_by_obj_desc(&ObjectDescriptor::GlobalIndex)
            .await
            .unwrap();

        match genesis_event.key() {
            KvKey::Empty { .. } => {
                panic!("Invalid state");
            }
            KvKey::Key { obj_id, .. } => {
                assert_eq!(obj_id.id_str(), tail_id.id_str());
            }
        }
    }
}
