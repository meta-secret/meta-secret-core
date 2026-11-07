//! Read tracing wraps actual storage; no business handler or synthetic success.
use anyhow::Result;
use async_trait::async_trait;
use meta_secret_core::node::db::{
    events::{generic_log_event::*, object_id::ArtifactId},
    in_mem_db::InMemKvLogEventRepo,
    repo::{generic_db::*, recovery_response_store::ServerRecoveryResponseStore},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
#[derive(Default)]
pub struct Repo {
    pub inner: Arc<InMemKvLogEventRepo>,
    pub writes: AtomicUsize,
    reads: Mutex<Vec<ArtifactId>>,
}
impl Repo {
    pub fn read_count(&self, id: &ArtifactId) -> usize {
        self.reads
            .lock()
            .unwrap()
            .iter()
            .filter(|found| *found == id)
            .count()
    }
    pub fn clear_reads(&self) {
        self.reads.lock().unwrap().clear();
    }
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.reads.lock().unwrap().push(id.clone());
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.reads.lock().unwrap().push(id.clone());
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.save(value).await
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.replace(value).await
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
