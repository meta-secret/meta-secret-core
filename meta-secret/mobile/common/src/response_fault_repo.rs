//! Private repository fault adapter for actual recovery-response tests.
use anyhow::{Result, bail};
use async_trait::async_trait;
use meta_secret_core::node::db::{
    events::{generic_log_event::*, object_id::ArtifactId, shared_secret_event::SsWorkflowObject},
    in_mem_db::InMemKvLogEventRepo,
    repo::generic_db::*,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
#[derive(Default)]
pub struct ResponseRepo {
    pub inner: Arc<InMemKvLogEventRepo>,
    pub fail_before: AtomicBool,
    pub fail_after: AtomicBool,
    pub fail_source_read: AtomicBool,
    pub target_read_error: Mutex<Option<ArtifactId>>,
    pub read_error_hits: AtomicUsize,
    pub saves: AtomicUsize,
    pub deletions: AtomicUsize,
}
#[async_trait(?Send)]
impl FindOneQuery for ResponseRepo {
    async fn find_one(&self, key: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        if self.target_read_error.lock().unwrap().as_ref() == Some(&key) {
            self.read_error_hits.fetch_add(1, Ordering::SeqCst);
            bail!("controlled fixed-slot read failure");
        }
        let found = self.inner.find_one(key).await?;
        if self.fail_source_read.load(Ordering::SeqCst)
            && matches!(
                &found,
                Some(GenericKvLogEvent::SsWorkflow(
                    SsWorkflowObject::Recovery(_) | SsWorkflowObject::Decline(_)
                ))
            )
        {
            bail!("controlled response read failure");
        }
        Ok(found)
    }
    async fn get_key(&self, key: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(key).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for ResponseRepo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let event = value.to_generic();
        let response = matches!(
            &event,
            GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_))
        );
        if response && self.fail_before.swap(false, Ordering::SeqCst) {
            bail!("controlled pre-save response failure");
        }
        let id = self.inner.save(event).await?;
        if response {
            self.saves.fetch_add(1, Ordering::SeqCst);
            if self.fail_after.swap(false, Ordering::SeqCst) {
                bail!("controlled post-save response loss");
            }
        }
        Ok(id)
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.inner.replace(value).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for ResponseRepo {
    async fn delete(&self, id: ArtifactId) {
        self.deletions.fetch_add(1, Ordering::SeqCst);
        self.inner.delete(id).await;
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for ResponseRepo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await;
    }
}
impl KvLogEventRepo for ResponseRepo {}
