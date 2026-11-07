//! Real storage forwarding with scoped automatic-Delete boundary faults.
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
pub(super) enum Step {
    Admission,
    Target,
    Terminal,
    Complete,
    Raw,
    Purge,
    Scrub,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    Before,
    Drop,
    Occupied,
    LostReply,
    Readback,
}
#[derive(Clone)]
struct Armed {
    step: Step,
    fault: Fault,
    source: ArtifactId,
    request_id: String,
}
pub(super) struct CorrelationRepo<R> {
    pub inner: Arc<R>,
    armed: Mutex<Option<Armed>>,
    last: Mutex<Option<(Step, ArtifactId)>>,
    pub hits: AtomicUsize,
    pub calls: Mutex<Vec<Step>>,
}
impl<R: KvLogEventRepo> CorrelationRepo<R> {
    pub fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            armed: Mutex::new(None),
            last: Mutex::new(None),
            hits: AtomicUsize::new(0),
            calls: Mutex::new(vec![]),
        }
    }
    pub fn arm(&self, source: &DeviceLogObject, id: &str, step: Step, fault: Fault) {
        *self.armed.lock().unwrap() = Some(Armed {
            step,
            fault,
            source: source.obj_id(),
            request_id: id.into(),
        });
        *self.last.lock().unwrap() = None;
        self.hits.store(0, Ordering::SeqCst);
        self.calls.lock().unwrap().clear();
    }
    pub fn clear(&self) {
        *self.armed.lock().unwrap() = None;
    }
    fn is(&self, step: Step, fault: Fault) -> bool {
        if self
            .armed
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|a| a.step == step && a.fault == fault)
        {
            self.hits.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
    async fn classify(&self, event: &GenericKvLogEvent, replace: bool) -> Result<Option<Step>> {
        let Some(a) = self.armed.lock().unwrap().clone() else {
            return Ok(None);
        };
        if replace
            && matches!(
                event,
                GenericKvLogEvent::DeviceLog(_) | GenericKvLogEvent::VaultLog(_)
            )
            && let Some(before) = self.inner.find_one(event.obj_id()).await?
            && transfer_count(&serde_json::to_value(before)?)
                > transfer_count(&serde_json::to_value(event)?)
        {
            return Ok(Some(Step::Scrub));
        }
        Ok(match event {
            GenericKvLogEvent::DeviceLog(_) if event.obj_id() == a.source => Some(Step::Raw),
            GenericKvLogEvent::Vault(v)
                if v.0.value.deletion_results.contains_key(&a.request_id) =>
            {
                Some(Step::Target)
            }
            GenericKvLogEvent::VaultLog(log) => {
                let wire = serde_json::to_value(&log.0.value)?;
                let complete = wire
                    .get("producerSourceRefs")
                    .and_then(Value::as_array)
                    .is_some_and(|refs| {
                        refs.iter().any(|r| {
                            r["requestId"] == a.request_id
                                && r["completionMode"] == "automaticComplete"
                        })
                    });
                if complete { Some(Step::Complete) }
                else if log.0.value.deletion_results.iter().any(|r| r.request.request_id == a.request_id) { Some(Step::Terminal) }
                else if log.0.value.requests.iter().any(|r| matches!(r, VaultActionRequestEvent::DeleteSecret(p) if p.request_id == a.request_id)) { Some(Step::Admission) }
                else { None }
            }
            _ => None,
        })
    }
    async fn write(&self, event: GenericKvLogEvent, replace: bool) -> Result<ArtifactId> {
        let step = self.classify(&event, replace).await?;
        if let Some(s) = step {
            self.calls.lock().unwrap().push(s);
            if self.is(s, Fault::Before) {
                anyhow::bail!("PRIVATE-CORRELATION-BACKEND before-write");
            }
            if self.is(s, Fault::Drop) {
                return Ok(event.obj_id());
            }
            if self.is(s, Fault::Occupied) {
                let mut different = event.clone();
                match &mut different {
                    GenericKvLogEvent::Vault(v) => {
                        v.0.value.secrets.insert(meta_secret_core::node::common::model::meta_pass::MetaPasswordId::build_from_str("unrelated-occupied-state"));
                    }
                    GenericKvLogEvent::VaultLog(v) => {
                        if !v.0.value.requests.is_empty() {
                            v.0.value.requests.clear();
                        } else {
                            v.0.value.deletion_results.clear();
                        }
                    }
                    _ => anyhow::bail!("unsupported occupied fixture"),
                }
                self.inner.save(different).await?;
            }
        }
        let id = if replace {
            self.inner.replace(event).await?
        } else {
            self.inner.save(event).await?
        };
        if let Some(s) = step {
            *self.last.lock().unwrap() = Some((s, id.clone()));
            if self.is(s, Fault::LostReply) {
                anyhow::bail!("PRIVATE-CORRELATION-BACKEND lost-response");
            }
        }
        Ok(id)
    }
}
pub(super) fn transfer_count(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.iter().map(transfer_count).sum(),
        Value::Object(map) => map
            .iter()
            .map(|(key, v)| {
                if key == "shareTransfers" {
                    v.as_array().map_or(0, Vec::len)
                } else {
                    transfer_count(v)
                }
            })
            .sum(),
        _ => 0,
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for CorrelationRepo<R> {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.write(value.to_generic(), false).await
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.write(value.to_generic(), true).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for CorrelationRepo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let value = self.inner.find_one(id.clone()).await?;
        if let Some((s, written)) = self.last.lock().unwrap().clone()
            && id == written
            && self.is(s, Fault::Readback)
        {
            anyhow::bail!("PRIVATE-CORRELATION-BACKEND readback");
        }
        Ok(value)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        Ok(self.find_one(id).await?.map(|v| v.obj_id()))
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for CorrelationRepo<R> {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await;
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        let workflow = self
            .inner
            .find_one(id.clone())
            .await?
            .is_some_and(|e| matches!(e, GenericKvLogEvent::SsWorkflow(_)));
        if workflow {
            self.calls.lock().unwrap().push(Step::Purge);
            if self.is(Step::Purge, Fault::Before) {
                anyhow::bail!("PRIVATE-CORRELATION-BACKEND purge");
            }
            if self.is(Step::Purge, Fault::Drop) {
                return Ok(());
            }
        }
        self.inner.try_delete(id.clone()).await?;
        if workflow {
            *self.last.lock().unwrap() = Some((Step::Purge, id));
            if self.is(Step::Purge, Fault::LostReply) {
                anyhow::bail!("PRIVATE-CORRELATION-BACKEND purge-response");
            }
        }
        Ok(())
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for CorrelationRepo<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await;
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for CorrelationRepo<R> {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.inner.server_recovery_response_store()
    }
}
