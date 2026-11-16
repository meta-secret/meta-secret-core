//! Only real storage forwarding, exact-marker faults and an actual save barrier.
use super::*;
use async_trait::async_trait;
use meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore;
use std::sync::{
    Mutex,
    atomic::{AtomicU8, AtomicUsize, Ordering},
};
use tokio::sync::Notify;

pub(super) struct Repo<R> {
    pub inner: Arc<R>,
    pub fault: AtomicU8,
    pub hits: AtomicUsize,
    pub writes: AtomicUsize,
    pub reads: Mutex<Vec<ArtifactId>>,
    last: Mutex<Option<ArtifactId>>,
    pause: Mutex<Option<ArtifactId>>,
    pub entered: Notify,
    pub resume: Notify,
}
impl<R> Repo<R> {
    pub fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            fault: AtomicU8::new(0),
            hits: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
            reads: Mutex::new(vec![]),
            last: Mutex::new(None),
            pause: Mutex::new(None),
            entered: Notify::new(),
            resume: Notify::new(),
        }
    }
    pub fn arm(&self, mode: u8) {
        self.fault.store(mode, Ordering::SeqCst);
        self.hits.store(0, Ordering::SeqCst);
    }
    pub fn pause_at(&self, id: ArtifactId) {
        *self.pause.lock().unwrap() = Some(id);
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for Repo<R> {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let event = value.to_generic();
        let paused = {
            let mut p = self.pause.lock().unwrap();
            if p.as_ref() == Some(&event.obj_id()) {
                p.take();
                true
            } else {
                false
            }
        };
        if paused {
            self.entered.notify_one();
            self.resume.notified().await;
        }
        let marker = matches!(event, GenericKvLogEvent::DeviceTransportNoop(_));
        let mode = self.fault.load(Ordering::SeqCst);
        if marker && mode != 0 {
            self.hits.fetch_add(1, Ordering::SeqCst);
        }
        if marker && mode == 1 {
            anyhow::bail!("B actual Noop before-save");
        }
        if marker && mode == 2 {
            return Ok(event.obj_id());
        }
        self.writes.fetch_add(1, Ordering::SeqCst);
        let id = self.inner.save(event).await?;
        if marker {
            *self.last.lock().unwrap() = Some(id.clone());
            if mode == 3 {
                anyhow::bail!("B actual Noop persisted/lost reply");
            }
        }
        Ok(id)
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.replace(value).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for Repo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.reads.lock().unwrap().push(id.clone());
        let found = self.inner.find_one(id.clone()).await?;
        if self.fault.load(Ordering::SeqCst) == 4 && self.last.lock().unwrap().as_ref() == Some(&id)
        {
            self.hits.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("PRIVATE-NOOP-BACKEND-SENTINEL persisted readback unavailable");
        }
        Ok(found)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        Ok(self.find_one(id).await?.map(|event| event.obj_id()))
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for Repo<R> {
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
impl<R: KvLogEventRepo> DbCleanUpCommand for Repo<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await;
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for Repo<R> {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.inner.server_recovery_response_store()
    }
}
