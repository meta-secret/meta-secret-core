//! Only actual repository operations are observed, blocked or failed.
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use meta_secret_core::node::db::{
    events::{
        generic_log_event::*,
        object_id::ArtifactId,
        vault::{device_log_event::DeviceLogObject, vault_log_event::*},
    },
    repo::{generic_db::*, recovery_response_store::ServerRecoveryResponseStore},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, AtomicUsize, Ordering},
};
use tokio::sync::Notify;

#[derive(Clone, Copy)]
pub enum Fault {
    RequestBefore = 1,
    RequestLostReply = 2,
    DecisionBefore = 3,
    DecisionLostReply = 4,
    MaterialBefore = 5,
    MaterialLostReply = 6,
}
#[derive(Default)]
pub struct Barrier {
    arm: AtomicU8,
    entered: Notify,
    release: Notify,
    id: Mutex<Option<ArtifactId>>,
}
impl Barrier {
    pub fn arm_request(&self) {
        self.arm.store(1, Ordering::SeqCst);
    }
    pub fn arm_read(&self) {
        self.arm.store(2, Ordering::SeqCst);
    }
    pub async fn wait(&self) {
        self.entered.notified().await;
    }
    pub fn release(&self) {
        self.release.notify_one();
    }
    pub fn id(&self) -> Result<ArtifactId> {
        self.id
            .lock()
            .unwrap()
            .clone()
            .context("actual saved producer R boundary")
    }
    async fn pause(&self, mode: u8, id: ArtifactId) {
        if self
            .arm
            .compare_exchange(mode, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            *self.id.lock().unwrap() = Some(id);
            self.entered.notify_one();
            self.release.notified().await;
        }
    }
}
pub struct Repo<R: KvLogEventRepo> {
    pub inner: Arc<R>,
    pub barrier: Barrier,
    mode: AtomicU8,
    pub writes: AtomicUsize,
    pub fault_hits: AtomicUsize,
    pub material_attempts: AtomicUsize,
    attempts: Mutex<Vec<DeviceLogObject>>,
}
impl<R: KvLogEventRepo> Repo<R> {
    pub fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            barrier: Barrier::default(),
            mode: AtomicU8::new(0),
            writes: AtomicUsize::new(0),
            fault_hits: AtomicUsize::new(0),
            material_attempts: AtomicUsize::new(0),
            attempts: Mutex::new(vec![]),
        }
    }
    pub fn arm(&self, fault: Fault) {
        self.mode.store(fault as u8, Ordering::SeqCst);
    }
    fn take(&self, mode: u8) -> bool {
        if self
            .mode
            .compare_exchange(mode, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.fault_hits.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
    pub fn sources(&self) -> Vec<DeviceLogObject> {
        self.attempts.lock().unwrap().clone()
    }
    pub fn reset_observation(&self) {
        self.attempts.lock().unwrap().clear();
        self.writes.store(0, Ordering::SeqCst);
        self.material_attempts.store(0, Ordering::SeqCst);
    }
}
fn producer_stage(event: &GenericKvLogEvent) -> Option<(DeviceLogObject, u8)> {
    let GenericKvLogEvent::DeviceLog(source) = event else {
        return None;
    };
    let stage = match &source.0.value {
        VaultActionEvent::Request(
            VaultActionRequestEvent::DeleteSecret(_) | VaultActionRequestEvent::RemoveMember(_),
        ) => 1,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(_)) => 3,
        _ => return None,
    };
    Some((source.clone(), stage))
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for Repo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let found = self.inner.find_one(id.clone()).await?;
        if matches!(found, Some(GenericKvLogEvent::Vault(_))) {
            self.barrier.pause(2, id).await;
        }
        Ok(found)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for Repo<R> {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let event = value.to_generic();
        let stage = producer_stage(&event);
        let material = matches!(&event, GenericKvLogEvent::SsDeviceLog(source)
            if source.clone().to_distribution_request().receivers.is_empty());
        let canonical_material = matches!(&event, GenericKvLogEvent::SsLog(log)
            if log.0.value.claims.values().any(|claim| claim.receivers.is_empty()));
        if material {
            self.material_attempts.fetch_add(1, Ordering::SeqCst);
            if self.take(5) {
                bail!("PRIVATE-PRODUCER-PERSISTENCE-ERROR material before actual write");
            }
        }
        if let Some((source, phase)) = &stage {
            self.attempts.lock().unwrap().push(source.clone());
            if self.take(*phase) {
                bail!("PRIVATE-PRODUCER-PERSISTENCE-ERROR before actual write");
            }
        }
        self.writes.fetch_add(1, Ordering::SeqCst);
        let id = self.inner.save(event).await?;
        if canonical_material && self.take(6) {
            bail!("PRIVATE-PRODUCER-PERSISTENCE-ERROR canonical material lost actual reply");
        }
        if let Some((_, phase)) = stage {
            if phase == 1 {
                self.barrier.pause(1, id.clone()).await;
            }
            if self.take(phase + 1) {
                bail!("PRIVATE-PRODUCER-PERSISTENCE-ERROR after actual write");
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
impl<R: KvLogEventRepo> DeleteCommand for Repo<R> {
    async fn delete(&self, id: ArtifactId) {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.delete(id).await
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
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for Repo<R> {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.inner.server_recovery_response_store()
    }
}
