//! Faults are injected around actual inner commits/readbacks; the private store is forwarded.
use super::*;
use async_trait::async_trait;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::db::repo::{
    generic_db::{DbCleanUpCommand, DeleteCommand, FindOneQuery},
    recovery_response_store::{
        PrivateRecoveryResponse, RecoveryResponseSourceProof, ServerRecoveryResponseStore,
    },
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Step {
    Reserve,
    Mark,
    Remove,
    Raw,
    Canonical,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Fault {
    Before,
    LostReply,
    Readback,
}

pub(super) struct ReceiptFaultRepo<R> {
    pub inner: Arc<R>,
    armed: Mutex<Option<(Step, Fault)>>,
    last: Mutex<Option<Step>>,
    pub hits: AtomicUsize,
    pub calls: Mutex<Vec<Step>>,
    supported: bool,
}
impl<R: KvLogEventRepo> ReceiptFaultRepo<R> {
    pub fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            armed: Mutex::new(None),
            last: Mutex::new(None),
            hits: AtomicUsize::new(0),
            calls: Mutex::new(vec![]),
            supported: true,
        }
    }
    pub fn unsupported(inner: Arc<R>) -> Self {
        let mut repo = Self::new(inner);
        repo.supported = false;
        repo
    }
    pub fn arm(&self, step: Step, fault: Fault) {
        *self.armed.lock().unwrap() = Some((step, fault));
    }
    pub fn clear(&self) {
        *self.armed.lock().unwrap() = None;
    }
    fn check(&self, step: Step, fault: Fault) -> Result<()> {
        if *self.armed.lock().unwrap() == Some((step, fault)) {
            self.hits.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("RecoveryResponseStore.Unavailable");
        }
        Ok(())
    }
    fn saved(&self, step: Step) {
        *self.last.lock().unwrap() = Some(step);
        self.calls.lock().unwrap().push(step);
    }
    fn event_step(event: &GenericKvLogEvent) -> Option<Step> {
        match event {
            GenericKvLogEvent::SsWorkflow(_) => Some(Step::Raw),
            GenericKvLogEvent::SsLog(log) if !log.0.value.liveboard_recovery_results.is_empty() => {
                Some(Step::Canonical)
            }
            _ => None,
        }
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> ServerRecoveryResponseStore for ReceiptFaultRepo<R> {
    async fn ensure_ready(&self) -> Result<()> {
        self.inner
            .server_recovery_response_store()?
            .ensure_ready()
            .await
    }
    async fn load(&self, vault: &VaultName) -> Result<Option<PrivateRecoveryResponse>> {
        let value = self
            .inner
            .server_recovery_response_store()?
            .load(vault)
            .await?;
        let last = *self.last.lock().unwrap();
        if let Some(step @ (Step::Reserve | Step::Mark | Step::Remove)) = last {
            self.check(step, Fault::Readback)?;
        }
        Ok(value)
    }
    async fn reserve(&self, vault: &VaultName, row: &PrivateRecoveryResponse) -> Result<()> {
        self.check(Step::Reserve, Fault::Before)?;
        self.inner
            .server_recovery_response_store()?
            .reserve(vault, row)
            .await?;
        self.saved(Step::Reserve);
        self.check(Step::Reserve, Fault::LostReply)
    }
    async fn mark_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()> {
        self.check(Step::Mark, Fault::Before)?;
        self.inner
            .server_recovery_response_store()?
            .mark_committed(vault, source)
            .await?;
        self.saved(Step::Mark);
        self.check(Step::Mark, Fault::LostReply)
    }
    async fn try_remove_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()> {
        self.check(Step::Remove, Fault::Before)?;
        self.inner
            .server_recovery_response_store()?
            .try_remove_committed(vault, source)
            .await?;
        self.saved(Step::Remove);
        self.check(Step::Remove, Fault::LostReply)
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for ReceiptFaultRepo<R> {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let value = value.to_generic();
        let step = Self::event_step(&value);
        if let Some(step) = step {
            self.check(step, Fault::Before)?;
        }
        let id = self.inner.save(value).await?;
        if let Some(step) = step {
            self.saved(step);
            self.check(step, Fault::LostReply)?;
        }
        Ok(id)
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.inner.replace(value).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for ReceiptFaultRepo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let value = self.inner.find_one(id).await?;
        if let Some(step) = value.as_ref().and_then(Self::event_step)
            && *self.last.lock().unwrap() == Some(step)
        {
            self.check(step, Fault::Readback)?;
        }
        Ok(value)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        Ok(self.find_one(id).await?.map(|e| e.obj_id()))
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for ReceiptFaultRepo<R> {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for ReceiptFaultRepo<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for ReceiptFaultRepo<R> {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        if !self.supported {
            return Err(meta_secret_core::node::db::repo::recovery_response_store::RecoveryResponseStoreError::UnsupportedBackend.into());
        }
        Ok(self)
    }
}
