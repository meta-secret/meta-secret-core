use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::global_index_descriptor::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ToObjectDescriptor};
use crate::node::db::events::generic_log_event::{
    ObjIdExtractor, ToGenericEvent, UnitEventWithEmptyValue,
};
use crate::node::db::events::global_index_event::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ObjectId, UnitId};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::anyhow;
use std::sync::Arc;
use tracing::{info, instrument};

pub struct ServerPersistentGlobalIndex<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub server_device: DeviceData,
}

impl<Repo: KvLogEventRepo> ServerPersistentGlobalIndex<Repo> {
    ///create a genesis event and save into the database
    #[instrument(skip(self))]
    pub async fn init(&self) -> anyhow::Result<()> {
        //Check if all required persistent objects has been created
        let gi_obj_desc = GlobalIndexDescriptor::Index.to_obj_desc();

        let maybe_unit_event = {
            let gi_unit = ObjectId::unit(gi_obj_desc.clone());
            self.p_obj.repo.find_one(gi_unit).await
        };

        let maybe_genesis_event = self
            .p_obj
            .repo
            .find_one(ObjectId::genesis(gi_obj_desc))
            .await;

        let gi_genesis_exists = matches!(maybe_unit_event, Ok(Some(_)));
        let gi_unit_exists = matches!(maybe_genesis_event, Ok(Some(_)));

        //If either of unit or genesis not exists then create initial records for the global index
        if gi_unit_exists && gi_genesis_exists {
            return Ok(());
        }

        info!("Init global index");

        let unit_event = GlobalIndexObject::unit().to_generic();
        let genesis_event = GlobalIndexObject::genesis(self.server_device.clone()).to_generic();

        self.p_obj.repo.save(unit_event.clone()).await?;
        self.p_obj.repo.save(genesis_event.clone()).await?;

        Ok(())
    }

    pub async fn update(&self, vault_name: VaultName) -> anyhow::Result<()> {
        //find the latest global_index_id???
        let gi_free_id = self
            .p_obj
            .find_free_id_by_obj_desc(ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index))
            .await?;

        let ObjectId::Artifact(gi_artifact_id) = gi_free_id else {
            return Err(anyhow!("Invalid global index state"));
        };

        let vault_id = UnitId::vault_unit(vault_name.clone());

        let gi_update_event = {
            GlobalIndexObject::Update(KvLogEvent {
                key: KvKey {
                    obj_id: gi_artifact_id,
                    obj_desc: GlobalIndexDescriptor::Index.to_obj_desc(),
                },
                value: vault_id.clone(),
            })
            .to_generic()
        };

        let gi_events = vec![gi_update_event];

        for gi_event in gi_events {
            self.p_obj.repo.save(gi_event).await?;
        }

        let vault_idx_evt = GlobalIndexObject::index_from_vault_id(vault_id).to_generic();

        self.p_obj.repo.save(vault_idx_evt).await?;

        anyhow::Ok(())
    }
}

pub struct ClientPersistentGlobalIndex<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> ClientPersistentGlobalIndex<Repo> {
    /// Save global index event and also save a "vault index" record
    pub async fn save(&self, gi_obj: &GlobalIndexObject) -> anyhow::Result<()> {
        self.p_obj.repo.save(gi_obj.clone().to_generic()).await?;

        // Update vault index according to global index
        if let GlobalIndexObject::Update(upd_event) = gi_obj {
            let vault_id = upd_event.value.clone();
            let vault_idx_evt = GlobalIndexObject::index_from_vault_id(vault_id).to_generic();
            self.p_obj.repo.save(vault_idx_evt).await?;
        }

        Ok(())
    }

    pub async fn not_exists(&self, vault_name: VaultName) -> anyhow::Result<bool> {
        let exists = self.exists(vault_name).await?;
        Ok(!exists)
    }

    pub async fn exists(&self, vault_name: VaultName) -> anyhow::Result<bool> {
        let vault_idx_obj = GlobalIndexObject::index_from_vault_name(vault_name).to_generic();
        let db_idx_event = self.p_obj.repo.get_key(vault_idx_obj.obj_id()).await?;
        Ok(db_idx_event.is_some())
    }
}

#[cfg(test)]
pub mod spec {
    use crate::node::common::model::device::common::DeviceData;
    use crate::node::db::descriptors::global_index_descriptor::GlobalIndexDescriptor;
    use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
    use crate::node::db::events::generic_log_event::ObjIdExtractor;
    use crate::node::db::events::global_index_event::GlobalIndexObject;
    use crate::node::db::events::object_id::ObjectId;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::global_index::fixture::ServerPersistentGlobalIndexFixture;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use std::sync::Arc;

    pub struct GlobalIndexSpec<Repo: KvLogEventRepo> {
        pub repo: Arc<Repo>,
        pub server_device: DeviceData,
    }

    impl<Repo: KvLogEventRepo> GlobalIndexSpec<Repo> {
        pub async fn verify(&self) -> anyhow::Result<()> {
            let gi_obj_desc = GlobalIndexDescriptor::Index.to_obj_desc();

            let unit_event = {
                let unit_id = ObjectId::unit(gi_obj_desc.clone());
                self.repo.find_one(unit_id).await?.unwrap().global_index()?
            };
            assert_eq!(unit_event.obj_id().get_unit_id().id.id, 0);

            let genesis_event = {
                let genesis_id = ObjectId::genesis(gi_obj_desc.clone());
                self.repo
                    .find_one(genesis_id)
                    .await?
                    .unwrap()
                    .global_index()?
            };

            if let GlobalIndexObject::Genesis(log_event) = genesis_event {
                assert_eq!(log_event.value, self.server_device);
            } else {
                panic!("Invalid Genesis event");
            }

            Ok(())
        }
    }

    impl From<ServerPersistentGlobalIndexFixture> for GlobalIndexSpec<InMemKvLogEventRepo> {
        fn from(gi_fixture: ServerPersistentGlobalIndexFixture) -> Self {
            Self {
                repo: gi_fixture.server_gi.p_obj.repo.clone(),
                server_device: gi_fixture.server_gi.server_device.clone(),
            }
        }
    }
}

#[cfg(test)]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::EmptyState;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::global_index::{
        ClientPersistentGlobalIndex, ServerPersistentGlobalIndex,
    };

    pub struct ServerPersistentGlobalIndexFixture {
        pub server_gi: ServerPersistentGlobalIndex<InMemKvLogEventRepo>,
        pub client_gi: ClientPersistentGlobalIndex<InMemKvLogEventRepo>,
    }

    impl From<&FixtureRegistry<EmptyState>> for ServerPersistentGlobalIndexFixture {
        fn from(registry: &FixtureRegistry<EmptyState>) -> Self {
            Self {
                server_gi: ServerPersistentGlobalIndex {
                    p_obj: registry.state.p_obj.server.clone(),
                    server_device: registry.state.device_creds.server.device.clone(),
                },
                client_gi: ClientPersistentGlobalIndex {
                    p_obj: registry.state.p_obj.client.clone(),
                },
            }
        }
    }

    impl ServerPersistentGlobalIndexFixture {
        pub fn generate() -> Self {
            ServerPersistentGlobalIndexFixture::from(&FixtureRegistry::empty())
        }
    }
}

#[cfg(test)]
mod test {
    use crate::node::db::objects::global_index::fixture::ServerPersistentGlobalIndexFixture;
    use crate::node::db::objects::global_index::spec::GlobalIndexSpec;

    #[tokio::test]
    async fn test_init() -> anyhow::Result<()> {
        let gi_fixture = ServerPersistentGlobalIndexFixture::generate();
        gi_fixture.server_gi.init().await?;

        let gi_spec = GlobalIndexSpec::from(gi_fixture);
        gi_spec.verify().await?;

        Ok(())
    }
}
