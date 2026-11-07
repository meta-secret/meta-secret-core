use anyhow::{Result, bail};
use async_trait::async_trait;
use meta_secret_core::node::db::{
    events::{generic_log_event::*, object_id::ArtifactId},
    in_mem_db::InMemKvLogEventRepo,
    repo::{generic_db::*, recovery_response_store::ServerRecoveryResponseStore},
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Default)]
pub struct Repo {
    inner: InMemKvLogEventRepo,
    pub fail_ss_log: AtomicBool,
    pub hits: AtomicUsize,
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
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        let event = event.to_generic();
        if matches!(event, GenericKvLogEvent::SsLog(_))
            && self.fail_ss_log.swap(false, Ordering::SeqCst)
        {
            self.hits.fetch_add(1, Ordering::SeqCst);
            bail!("test-only canonical claim save failure");
        }
        self.inner.save(event).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.inner.replace(event).await
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
impl KvLogEventRepo for Repo {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.inner.server_recovery_response_store()
    }
}
