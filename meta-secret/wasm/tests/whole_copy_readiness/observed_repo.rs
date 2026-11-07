//! Only real local writes are counted; the actual two-member Vault save can pause.
use anyhow::Result;
use async_trait::async_trait;
use meta_secret_core::node::db::{
    events::{generic_log_event::*, object_id::ArtifactId},
    in_mem_db::InMemKvLogEventRepo,
    repo::{generic_db::*, recovery_response_store::ServerRecoveryResponseStore},
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::sync::Notify;
#[derive(Default)]
pub struct Repo {
    pub inner: Arc<InMemKvLogEventRepo>,
    pub writes: AtomicUsize,
    armed: AtomicBool,
    entered: Notify,
    released: Notify,
}
impl Repo {
    pub fn arm_join(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }
    pub async fn wait_join(&self) {
        self.entered.notified().await;
    }
    pub fn release_join(&self) {
        self.released.notify_one();
    }
    pub async fn snapshot(&self) -> HashMap<ArtifactId, GenericKvLogEvent> {
        self.inner.get_db().await
    }
    async fn write(&self, event: GenericKvLogEvent, replace: bool) -> Result<ArtifactId> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        let pause = matches!(&event,GenericKvLogEvent::Vault(v) if v.0.value.members().len()==2)
            && self.armed.swap(false, Ordering::SeqCst);
        let id = if replace {
            self.inner.replace(event).await?
        } else {
            self.inner.save(event).await?
        };
        if pause {
            self.entered.notify_one();
            self.released.notified().await;
        }
        Ok(id)
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
        self.write(value.to_generic(), false).await
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.write(value.to_generic(), true).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Repo {
    async fn delete(&self, id: ArtifactId) {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.delete(id).await;
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Repo {
    async fn db_clean_up(&self) {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.db_clean_up().await;
    }
}
impl KvLogEventRepo for Repo {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.inner.server_recovery_response_store()
    }
}
