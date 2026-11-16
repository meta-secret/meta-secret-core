use super::*;

#[derive(Clone)]
pub(super) enum Operation {
    Find(ArtifactId),
    Key(ArtifactId),
    Write(bool, Box<GenericKvLogEvent>),
    Delete(ArtifactId),
    TryDelete(ArtifactId),
    Clean,
    Recovery,
}
#[derive(Clone)]
pub(super) struct Call {
    pub order: usize,
    pub operation: Operation,
}
#[derive(Clone)]
pub(super) struct Read {
    pub order: usize,
    pub id: ArtifactId,
    pub event: Option<GenericKvLogEvent>,
}
#[derive(Debug)]
struct WitnessReadError;
impl std::fmt::Display for WitnessReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("selected witness read unavailable")
    }
}
impl std::error::Error for WitnessReadError {}
pub(super) struct Repo {
    pub physical: Arc<InMemKvLogEventRepo>,
    pub clock: Arc<AtomicUsize>,
    pub calls: Mutex<Vec<Call>>,
    pub reads: Mutex<Vec<Read>>,
    pub read_fault: Mutex<Option<ArtifactId>>,
    pub read_hits: AtomicUsize,
}
impl Repo {
    pub fn new(physical: Arc<InMemKvLogEventRepo>) -> Self {
        Self {
            physical,
            clock: Arc::new(0.into()),
            calls: Mutex::new(vec![]),
            reads: Mutex::new(vec![]),
            read_fault: Mutex::new(None),
            read_hits: 0.into(),
        }
    }
    pub fn tick(&self) -> usize {
        self.clock.fetch_add(1, Ordering::SeqCst) + 1
    }
    fn call(&self, operation: Operation) -> usize {
        let order = self.tick();
        self.calls.lock().unwrap().push(Call { order, operation });
        order
    }
    pub fn reset(&self) {
        self.calls.lock().unwrap().clear();
        self.reads.lock().unwrap().clear();
    }
    async fn write(&self, replace: bool, e: GenericKvLogEvent) -> Result<ArtifactId> {
        self.call(Operation::Write(replace, Box::new(e.clone())));
        if replace {
            self.physical.replace(e).await
        } else {
            self.physical.save(e).await
        }
    }
    pub fn exact_read_before(&self, event: &GenericKvLogEvent, order: usize) -> bool {
        self.reads
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.order < order && r.id == event.obj_id() && r.event.as_ref() == Some(event))
    }
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.call(Operation::Find(id.clone()));
        if self.read_fault.lock().unwrap().as_ref() == Some(&id) {
            self.read_hits.fetch_add(1, Ordering::SeqCst);
            return Err(anyhow::Error::new(WitnessReadError)); // BEFORE forwarding, never a false successful read.
        }
        let event = self.physical.find_one(id.clone()).await?;
        let order = self.tick(); // Successful OUTER observation occurs after the actual await.
        self.reads.lock().unwrap().push(Read {
            order,
            id,
            event: event.clone(),
        });
        Ok(event)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.call(Operation::Key(id.clone()));
        self.physical.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, e: T) -> Result<ArtifactId> {
        self.write(false, e.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, e: T) -> Result<ArtifactId> {
        self.write(true, e.to_generic()).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Repo {
    async fn delete(&self, id: ArtifactId) {
        self.call(Operation::Delete(id.clone()));
        self.physical.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.call(Operation::TryDelete(id.clone()));
        self.physical.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Repo {
    async fn db_clean_up(&self) {
        self.call(Operation::Clean);
        self.physical.db_clean_up().await
    }
}
impl KvLogEventRepo for Repo {
    fn server_recovery_response_store(
        &self,
    ) -> Result<
        &dyn meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore,
    > {
        self.call(Operation::Recovery);
        self.physical.server_recovery_response_store()
    }
}
