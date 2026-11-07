use super::*;
#[derive(Clone)]
pub(super) enum Op {
    Find(ArtifactId),
    Key(ArtifactId),
    Write(bool, Box<GenericKvLogEvent>),
    Delete(ArtifactId),
    TryDelete(ArtifactId),
    Clean,
    Recovery,
}
pub(super) struct Call {
    pub order: usize,
    pub op: Op,
}
pub(super) struct Read {
    pub order: usize,
    pub event: Option<GenericKvLogEvent>,
}
pub(super) struct Write {
    pub forward: usize,
    pub done: usize,
    pub event: GenericKvLogEvent,
    pub success: bool,
    pub exact: bool,
}
pub(super) struct Repo {
    pub inner: Arc<Inner>,
    pub physical: Arc<InMemKvLogEventRepo>,
    pub calls: Mutex<Vec<Call>>,
    pub reads: Mutex<Vec<Read>>,
    pub writes: Mutex<Vec<Write>>,
    clock: AtomicUsize,
}
impl Repo {
    pub fn new(case: &Case, local: &local_repo::Repo) -> Self {
        Self {
            inner: case.h.p.repo.clone(),
            physical: local.inner.clone(),
            calls: Mutex::new(vec![]),
            reads: Mutex::new(vec![]),
            writes: Mutex::new(vec![]),
            clock: 0.into(),
        }
    }
    pub fn tick(&self) -> usize {
        self.clock.fetch_add(1, Ordering::SeqCst) + 1
    }
    fn call(&self, op: Op) -> usize {
        let order = self.tick();
        self.calls.lock().unwrap().push(Call { order, op });
        order
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        self.call(Op::Write(replace, Box::new(event.clone())));
        let forward = self.tick();
        let result = if replace {
            self.inner.replace(event.clone()).await
        } else {
            self.inner.save(event.clone()).await
        };
        let done = self.tick();
        let exact = self.physical.find_one(event.obj_id()).await?.as_ref() == Some(&event);
        self.writes.lock().unwrap().push(Write {
            forward,
            done,
            event,
            success: result.is_ok(),
            exact,
        });
        result
    }
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.call(Op::Find(id.clone()));
        let event = self.inner.find_one(id).await?;
        self.reads.lock().unwrap().push(Read {
            order: self.tick(),
            event: event.clone(),
        });
        Ok(event)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.call(Op::Key(id.clone()));
        self.inner.get_key(id).await
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
        self.call(Op::Delete(id.clone()));
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.call(Op::TryDelete(id.clone()));
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Repo {
    async fn db_clean_up(&self) {
        self.call(Op::Clean);
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for Repo {
    fn server_recovery_response_store(
        &self,
    ) -> Result<
        &dyn meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore,
    > {
        self.call(Op::Recovery);
        self.inner.server_recovery_response_store()
    }
}
