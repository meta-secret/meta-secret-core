use super::*;

#[derive(Clone)]
pub(super) enum Operation {
    Write(GenericKvLogEvent),
    Read(ArtifactId, Option<GenericKvLogEvent>),
    Delete(ArtifactId),
    TryDelete(ArtifactId),
}

#[derive(Debug)]
pub(super) struct TargetDeleteBefore;
impl std::fmt::Display for TargetDeleteBefore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("legacy target delete before boundary")
    }
}
impl std::error::Error for TargetDeleteBefore {}

pub(super) struct Observed {
    pub inner: Arc<InMemKvLogEventRepo>,
    pub operations: Mutex<Vec<Operation>>,
    target_fault: Mutex<Option<ArtifactId>>,
    pub fault_hits: AtomicUsize,
}
impl Observed {
    pub fn new(inner: Arc<InMemKvLogEventRepo>) -> Self {
        Self {
            inner,
            operations: Mutex::new(vec![]),
            target_fault: Mutex::new(None),
            fault_hits: 0.into(),
        }
    }
    pub fn reset(&self) {
        self.operations.lock().unwrap().clear();
    }
    pub fn arm(&self, target: ArtifactId) {
        *self.target_fault.lock().unwrap() = Some(target);
    }
    pub fn clear(&self) {
        *self.target_fault.lock().unwrap() = None;
    }
    pub fn writes(&self) -> Vec<GenericKvLogEvent> {
        self.operations
            .lock()
            .unwrap()
            .iter()
            .filter_map(|op| match op {
                Operation::Write(event) => Some(event.clone()),
                _ => None,
            })
            .collect()
    }
    pub fn material_attempts(&self) -> usize {
        self.writes()
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_))
                        | GenericKvLogEvent::SsDeviceLog(_)
                        | GenericKvLogEvent::SsLog(_)
                )
            })
            .count()
    }
}

#[async_trait(?Send)]
impl FindOneQuery for Observed {
    async fn find_one(&self, key: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let result = self.inner.find_one(key.clone()).await?;
        self.operations
            .lock()
            .unwrap()
            .push(Operation::Read(key, result.clone()));
        Ok(result)
    }
    async fn get_key(&self, key: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(key).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Observed {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        let event = event.to_generic();
        self.operations
            .lock()
            .unwrap()
            .push(Operation::Write(event.clone()));
        self.inner.save(event).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        let event = event.to_generic();
        self.operations
            .lock()
            .unwrap()
            .push(Operation::Write(event.clone()));
        self.inner.replace(event).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Observed {
    async fn delete(&self, key: ArtifactId) {
        self.operations
            .lock()
            .unwrap()
            .push(Operation::Delete(key.clone()));
        self.inner.delete(key).await
    }
    async fn try_delete(&self, key: ArtifactId) -> Result<()> {
        self.operations
            .lock()
            .unwrap()
            .push(Operation::TryDelete(key.clone()));
        if self.target_fault.lock().unwrap().as_ref() == Some(&key) {
            self.fault_hits.fetch_add(1, Ordering::SeqCst);
            return Err(anyhow::Error::new(TargetDeleteBefore));
        }
        self.inner.try_delete(key).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Observed {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for Observed {}
