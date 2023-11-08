use std::sync::Arc;

use crate::node::db::events::object_id::{Next, ObjectId};
use crate::node::db::generic_db::{FindOneQuery, KvLogEventRepo};

pub struct PersistentObjectNavigator<Repo: KvLogEventRepo> {
    repo: Arc<Repo>,
    obj_id: ObjectId,
}

impl<Repo: KvLogEventRepo> PersistentObjectNavigator<Repo> {
    pub async fn build(repo: Arc<Repo>, obj_id: ObjectId) -> PersistentObjectNavigator<Repo> {
        PersistentObjectNavigator {
            repo: repo.clone(),
            obj_id,
        }
    }

    pub async fn next(&mut self) -> anyhow::Result<Option<ObjectId>> {
        let maybe_key = self.repo.get_key(self.obj_id.clone()).await?;

        if let Some(obj_id) = maybe_key {
            self.obj_id = obj_id.next();
            Ok(Some(obj_id.clone()))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::crypto::keys::{KeyManager, OpenBox};
    use crate::node::db::events::common::PublicKeyRecord;
    use crate::node::db::events::generic_log_event::{ObjIdExtractor, ToGenericEvent, UnitEventWithEmptyValue};
    use crate::node::db::events::global_index::GlobalIndexObject;
    use crate::node::db::generic_db::SaveCommand;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object_navigator::PersistentObjectNavigator;

    #[tokio::test]
    async fn test_iterator() -> anyhow::Result<()> {
        let repo = Arc::new(InMemKvLogEventRepo::default());

        let server_pk = {
            let secret_box = KeyManager::generate_secret_box();
            let open_box = OpenBox::from(&secret_box);
            PublicKeyRecord::from(open_box.dsa_pk)
        };

        let unit_event = GlobalIndexObject::unit().to_generic();
        let genesis_event = GlobalIndexObject::genesis(server_pk).to_generic();

        repo.save(unit_event).await?;
        repo.save(genesis_event).await?;

        let mut navigator = PersistentObjectNavigator::build(repo, unit_event.obj_id()).await;
        assert_eq!(Some(unit_event.obj_id()), navigator.next().await?);
        assert_eq!(Some(genesis_event.obj_id()), navigator.next().await?);

        Ok(())
    }
}
