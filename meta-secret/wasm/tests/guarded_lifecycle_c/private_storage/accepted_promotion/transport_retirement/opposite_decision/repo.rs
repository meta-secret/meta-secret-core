use super::*;
use meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore;

/// Pure outer forwarding observation; physical probes never become production readbacks.
pub(super) struct Outer<R: KvLogEventRepo> {
    pub inner: Arc<ActualRepo<R>>,
    pub physical: Arc<R>,
    pub clock: Arc<AtomicUsize>,
    pub methods: Mutex<Vec<&'static str>>,
    pub writes: Mutex<Vec<Write>>,
    pub reads: Mutex<Vec<Read>>,
}
impl<R: KvLogEventRepo> Outer<R> {
    pub fn new(case: &ActualCase<R>, physical: Arc<R>) -> Self {
        Self {
            inner: case.h.p.repo.clone(),
            physical,
            clock: Arc::new(0.into()),
            methods: Mutex::new(vec![]),
            writes: Mutex::new(vec![]),
            reads: Mutex::new(vec![]),
        }
    }
    fn call(&self, m: &'static str) {
        self.methods.lock().unwrap().push(m);
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        self.writes.lock().unwrap().push(Write {
            order: tick(&self.clock),
            replace,
            event: event.clone(),
        });
        if replace {
            self.inner.replace(event).await
        } else {
            self.inner.save(event).await
        }
    }
    pub fn witness_read(&self, auth: &DeviceTransportNoop, before: usize) -> bool {
        self.reads.lock().unwrap().iter().any(|r| {
            r.order < before
                && matches!(
            &r.event,GenericKvLogEvent::LocalGuardedPreparation(w)
            if w.0.value.transport_noop.as_ref().is_some_and(|n|n.authorization==*auth))
        })
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for Outer<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.call("find_one");
        let result = self.inner.find_one(id).await?;
        if let Some(e) = &result {
            self.reads.lock().unwrap().push(Read {
                order: tick(&self.clock),
                event: e.clone(),
            });
        }
        Ok(result)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.call("get_key");
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for Outer<R> {
    async fn save<T: ToGenericEvent>(&self, e: T) -> Result<ArtifactId> {
        self.call("save");
        self.write(false, e.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, e: T) -> Result<ArtifactId> {
        self.call("replace");
        self.write(true, e.to_generic()).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for Outer<R> {
    async fn delete(&self, id: ArtifactId) {
        self.call("delete");
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.call("try_delete");
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for Outer<R> {
    async fn db_clean_up(&self) {
        self.call("db_clean_up");
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for Outer<R> {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.call("server_recovery_response_store");
        self.inner.server_recovery_response_store()
    }
}
