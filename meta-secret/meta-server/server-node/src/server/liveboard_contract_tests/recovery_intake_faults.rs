use super::*;
use async_trait::async_trait;
use meta_secret_core::node::db::{
    events::generic_log_event::GenericKvLogEvent,
    repo::{
        generic_db::{DbCleanUpCommand, DeleteCommand, FindOneQuery},
        recovery_response_store::ServerRecoveryResponseStore,
    },
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Canonical,
    Raw,
    Cleanup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    Before,
    LostReply,
    Drop,
    WrongKey,
    Readback,
}
pub(super) struct IntakeFaultRepo<R> {
    pub inner: Arc<R>,
    armed: Mutex<Option<(Phase, Fault)>>,
    last: Mutex<Option<(Phase, ArtifactId)>>,
    pub hits: AtomicUsize,
    pub read_hits: AtomicUsize,
    pub calls: Mutex<Vec<Phase>>,
}
impl<R: KvLogEventRepo> IntakeFaultRepo<R> {
    pub fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            armed: Mutex::new(None),
            last: Mutex::new(None),
            hits: AtomicUsize::new(0),
            read_hits: AtomicUsize::new(0),
            calls: Mutex::new(vec![]),
        }
    }
    pub fn arm(&self, phase: Phase, fault: Fault) {
        *self.armed.lock().unwrap() = Some((phase, fault));
        *self.last.lock().unwrap() = None;
        self.calls.lock().unwrap().clear();
        self.hits.store(0, Ordering::SeqCst);
        self.read_hits.store(0, Ordering::SeqCst);
    }
    pub fn clear(&self) {
        *self.armed.lock().unwrap() = None;
    }
    fn is(&self, phase: Phase, fault: Fault) -> bool {
        if *self.armed.lock().unwrap() == Some((phase, fault)) {
            self.hits.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
    async fn write(&self, event: GenericKvLogEvent, replace: bool) -> Result<ArtifactId> {
        let phase = if replace && matches!(event, GenericKvLogEvent::SsLog(_)) {
            Some(Phase::Cleanup)
        } else {
            match &event {
                GenericKvLogEvent::SsLog(_) => Some(Phase::Canonical),
                GenericKvLogEvent::SsDeviceLog(_) => Some(Phase::Raw),
                _ => None,
            }
        };
        if let Some(p) = phase {
            self.calls.lock().unwrap().push(p);
            if self.is(p, Fault::Before) {
                anyhow::bail!("RecoveryRequestIntake.Unavailable");
            }
            if self.is(p, Fault::Drop) {
                return Ok(event.obj_id());
            }
        }
        let id = if replace {
            self.inner.replace(event).await?
        } else {
            self.inner.save(event).await?
        };
        if let Some(p) = phase {
            *self.last.lock().unwrap() = Some((p, id.clone()));
            if self.is(p, Fault::LostReply) {
                anyhow::bail!("RecoveryRequestIntake.Unavailable");
            }
            if self.is(p, Fault::WrongKey) {
                return Ok(id.next());
            }
        }
        Ok(id)
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for IntakeFaultRepo<R> {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(event.to_generic(), false).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(event.to_generic(), true).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for IntakeFaultRepo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let value = self.inner.find_one(id.clone()).await?;
        let last = self.last.lock().unwrap().clone();
        if let Some((phase, written)) = last
            && id == written
            && self.is(phase, Fault::Readback)
        {
            self.read_hits.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("RecoveryRequestIntake.Unavailable");
        }
        Ok(value)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        Ok(self.find_one(id).await?.map(|event| event.obj_id()))
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for IntakeFaultRepo<R> {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for IntakeFaultRepo<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for IntakeFaultRepo<R> {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.inner.server_recovery_response_store()
    }
}
