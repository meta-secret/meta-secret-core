use anyhow::{Result, bail};
use async_trait::async_trait;
use meta_secret_core::node::db::{
    events::{generic_log_event::*, object_id::ArtifactId},
    in_mem_db::InMemKvLogEventRepo,
    repo::generic_db::*,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU8, AtomicUsize, Ordering},
};

#[derive(Clone, Copy)]
pub enum Fault {
    BeforeDeviceLog = 1,
    LostDeviceLogReply = 2,
    BeforeSsDeviceLog = 3,
    LostSsDeviceLogReply = 4,
    BeforeDistribution = 5,
}
#[derive(Default)]
pub struct Repo {
    inner: InMemKvLogEventRepo,
    mode: AtomicU8,
    pub hits: AtomicUsize,
}
impl Repo {
    pub fn arm(&self, fault: Fault) {
        self.mode.store(fault as u8, Ordering::SeqCst);
    }
    pub async fn get_db(&self) -> HashMap<ArtifactId, GenericKvLogEvent> {
        self.inner.get_db().await
    }
    fn take(&self, kind: u8) -> bool {
        if self
            .mode
            .compare_exchange(kind, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.hits.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let event = value.to_generic();
        let device = matches!(event, GenericKvLogEvent::DeviceLog(_));
        let ss = matches!(event, GenericKvLogEvent::SsDeviceLog(_));
        let distribution = matches!(event, GenericKvLogEvent::SsWorkflow(_));
        if (device && self.take(Fault::BeforeDeviceLog as u8))
            || (ss && self.take(Fault::BeforeSsDeviceLog as u8))
            || (distribution && self.take(Fault::BeforeDistribution as u8))
        {
            bail!("injected pre-persist failure PRIVATE-ERROR-CONTENT");
        }
        let id = self.inner.save(event).await?;
        if (device && self.take(Fault::LostDeviceLogReply as u8))
            || (ss && self.take(Fault::LostSsDeviceLogReply as u8))
        {
            bail!("injected post-persist response loss PRIVATE-ERROR-CONTENT");
        }
        Ok(id)
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.inner.replace(value).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Repo {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Repo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for Repo {}
