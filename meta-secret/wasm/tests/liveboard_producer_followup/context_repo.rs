//! Actual local backend faults; no receipt/readiness/domain response is synthesized.
use anyhow::{Result, bail};
use async_trait::async_trait;
use meta_secret_core::node::{
    common::model::liveboard_producer::{ProducerMaterialStage, ProducerSourceObservation},
    db::{
        events::{generic_log_event::*, object_id::ArtifactId},
        repo::{generic_db::*, recovery_response_store::ServerRecoveryResponseStore},
    },
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, AtomicUsize, Ordering},
};

#[derive(Clone, Copy)]
pub enum Fault {
    ClearBefore = 1,
    TerminalContextBefore = 2,
    ResolvedMaterialLostReply = 3,
    CanonicalMaterialLostReply = 4,
}
pub struct Repo<R: KvLogEventRepo> {
    pub inner: Arc<R>,
    mode: AtomicU8,
    pub hits: AtomicUsize,
    pub material_writes: AtomicUsize,
    pub writes: AtomicUsize,
    pub resolved_readbacks: AtomicUsize,
    lost_material: Mutex<Option<GenericKvLogEvent>>,
    pub singleton_canonical_saves: AtomicUsize,
}
impl<R: KvLogEventRepo> Repo<R> {
    pub fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            mode: AtomicU8::new(0),
            hits: AtomicUsize::new(0),
            material_writes: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
            resolved_readbacks: AtomicUsize::new(0),
            lost_material: Mutex::new(None),
            singleton_canonical_saves: AtomicUsize::new(0),
        }
    }
    pub fn arm(&self, fault: Fault) {
        self.mode.store(fault as u8, Ordering::SeqCst);
    }
    fn take(&self, fault: Fault) -> bool {
        if self
            .mode
            .compare_exchange(fault as u8, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.hits.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
    pub fn reset(&self) {
        self.material_writes.store(0, Ordering::SeqCst);
        self.writes.store(0, Ordering::SeqCst);
        self.singleton_canonical_saves.store(0, Ordering::SeqCst);
    }
    async fn write(&self, event: GenericKvLogEvent, replace: bool) -> Result<ArtifactId> {
        if let GenericKvLogEvent::LocalRemovalEnvelope(record) = &event {
            if record.0.value.producer_context.is_none() && self.take(Fault::ClearBefore) {
                bail!("PRIVATE-FOLLOWUP clear before actual write");
            }
            if record.0.value.producer_context.as_ref().is_some_and(|c| {
                c.material_stage == ProducerMaterialStage::Ready
                    && c.request_id.is_some()
                    && c.request_stage.observation == ProducerSourceObservation::RawAcknowledged
                    && c.decision_stage.as_ref().is_some_and(|s| {
                        s.observation == ProducerSourceObservation::RawAcknowledged
                    })
            }) && self.take(Fault::TerminalContextBefore)
            {
                bail!("PRIVATE-FOLLOWUP terminal context before actual write");
            }
        }
        let local_material = matches!(&event, GenericKvLogEvent::SsDeviceLog(source)
            if source.0.value.receivers.is_empty());
        let canonical_material = matches!(&event, GenericKvLogEvent::SsLog(log)
            if log.0.value.claims.values().any(|claim| claim.receivers.is_empty()));
        if matches!(
            event,
            GenericKvLogEvent::SsDeviceLog(_)
                | GenericKvLogEvent::SsLog(_)
                | GenericKvLogEvent::SsWorkflow(_)
        ) {
            self.material_writes.fetch_add(1, Ordering::SeqCst);
        }
        self.writes.fetch_add(1, Ordering::SeqCst);
        let id = if replace {
            self.inner.replace(event.clone()).await?
        } else {
            self.inner.save(event.clone()).await?
        };
        if canonical_material {
            self.singleton_canonical_saves
                .fetch_add(1, Ordering::SeqCst);
        }
        if local_material && self.take(Fault::ResolvedMaterialLostReply) {
            *self.lost_material.lock().unwrap() = Some(event);
            bail!("PRIVATE-FOLLOWUP actual local material saved, reply lost");
        }
        if canonical_material && self.take(Fault::CanonicalMaterialLostReply) {
            bail!("PRIVATE-FOLLOWUP actual canonical material saved, reply lost");
        }
        Ok(id)
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for Repo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let actual = self.inner.find_one(id).await?;
        if let Some(expected) = self.lost_material.lock().unwrap().as_ref()
            && actual.as_ref() == Some(expected)
        {
            self.resolved_readbacks.fetch_add(1, Ordering::SeqCst);
        }
        Ok(actual)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for Repo<R> {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.write(value.to_generic(), false).await
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.write(value.to_generic(), true).await
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
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.db_clean_up().await;
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for Repo<R> {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.inner.server_recovery_response_store()
    }
}
