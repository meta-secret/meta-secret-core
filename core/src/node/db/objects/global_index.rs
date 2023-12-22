use std::sync::Arc;
use async_trait::async_trait;
use tracing_attributes::instrument;
use tracing::info;
use crate::node::common::model::device::DeviceData;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::repo::generic_db::{KvLogEventRepo, SaveCommand};

#[async_trait(? Send)]
pub trait PersistentGlobalIndexApi {
    async fn init(&self, public_key: DeviceData) -> anyhow::Result<Vec<GenericKvLogEvent>>;
}

pub struct PersistentGlobalIndex<Repo: KvLogEventRepo> {
    pub repo: Arc<Repo>,
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo> PersistentGlobalIndexApi for PersistentGlobalIndex<Repo> {

    ///create a genesis event and save into the database
    #[instrument(skip(self))]
    async fn init(&self, public_key: DeviceData) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        info!("Init global index");

        let unit_event = GlobalIndexObject::Unit(KvLogEvent::global_index_unit())
            .to_generic();
        let genesis_event = GlobalIndexObject::Genesis(KvLogEvent::global_index_genesis(public_key))
            .to_generic();

        self.repo.save(unit_event.clone()).await?;
        self.repo.save(genesis_event.clone()).await?;


        Ok(vec![unit_event, genesis_event])
    }
}
