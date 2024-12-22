use crate::node::common::model::device::common::DeviceData;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::objects::global_index::ClientPersistentGlobalIndex;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::GlobalIndexRequest;
use anyhow::Result;
use std::sync::Arc;

pub struct GlobalIndexDbSync<Repo: KvLogEventRepo> {
    p_obj: Arc<PersistentObject<Repo>>,
    sender: DeviceData,
    p_gi: ClientPersistentGlobalIndex<Repo>,
}

impl<Repo: KvLogEventRepo> GlobalIndexDbSync<Repo> {
    pub fn new(p_obj: Arc<PersistentObject<Repo>>, sender: DeviceData) -> Self {
        let p_gi = ClientPersistentGlobalIndex {
            p_obj: p_obj.clone(),
        };

        Self {
            p_obj,
            sender,
            p_gi,
        }
    }
}

impl<Repo: KvLogEventRepo> GlobalIndexDbSync<Repo> {
    pub async fn save(&self, gi_events: Vec<GenericKvLogEvent>) -> Result<()> {
        for gi_event in gi_events {
            let gi_obj = gi_event.global_index()?;
            self.p_gi.save(&gi_obj).await?;
        }

        Ok(())
    }
}

impl<Repo: KvLogEventRepo> GlobalIndexDbSync<Repo> {
    pub async fn get_gi_request(&self) -> Result<GlobalIndexRequest> {
        let gi_free_id = self.p_gi.free_id().await?;

        Ok(GlobalIndexRequest {
            sender: self.sender.clone(),
            global_index: gi_free_id,
        })
    }
}
