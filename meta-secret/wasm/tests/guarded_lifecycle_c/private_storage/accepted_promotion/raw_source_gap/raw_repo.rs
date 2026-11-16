use super::*;
use async_trait::async_trait;
use meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore;

#[derive(Clone)]
struct Plan {
    expected: Expected,
    body: GenericKvLogEvent,
    fault: Fault,
    observations: Arc<Observations>,
}
pub(super) struct RawRepo {
    pub inner: Arc<InMemKvLogEventRepo>,
    plan: Mutex<Option<Plan>>,
    pub armed: AtomicBool,
    pub hits: AtomicUsize,
    pub applied_before_fault: AtomicBool,
    pub raw_reads: AtomicUsize,
}
impl RawRepo {
    pub fn new(inner: Arc<InMemKvLogEventRepo>) -> Self {
        Self {
            inner,
            plan: Mutex::new(None),
            armed: false.into(),
            hits: 0.into(),
            applied_before_fault: false.into(),
            raw_reads: 0.into(),
        }
    }
    pub fn arm(
        &self,
        expected: Expected,
        body: GenericKvLogEvent,
        fault: Fault,
        observations: Arc<Observations>,
    ) {
        *self.plan.lock().unwrap() = Some(Plan {
            expected,
            body,
            fault,
            observations,
        });
        self.armed.store(true, Ordering::SeqCst);
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        let plan = self.plan.lock().unwrap().clone();
        let selected = plan.as_ref().filter(|p| {
            self.armed.load(Ordering::SeqCst)
                && event == p.body
                && event.obj_id() == p.expected.event_id
        });
        if let Some(p) = selected {
            let action = p
                .observations
                .action()
                .context("actual authenticated source before server raw save")?;
            // Read SAME backing canonical store: unique exact result/source and actual members2.
            let applied = proof::applied(self.inner.clone(), &p.expected, &action).await?;
            self.applied_before_fault.store(applied, Ordering::SeqCst);
            ensure!(
                applied,
                "raw fault cannot manufacture canonical Applied proof"
            );
            self.hits.fetch_add(1, Ordering::SeqCst);
            if p.fault == Fault::Drop {
                return Ok(event.obj_id());
            }
        }
        let saved = if replace {
            self.inner.replace(event).await?
        } else {
            self.inner.save(event).await?
        };
        if selected.is_some() {
            anyhow::bail!("actual server raw durable lost reply PRIVATE_SENTINEL");
        }
        Ok(saved)
    }
}
#[async_trait(?Send)]
impl FindOneQuery for RawRepo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        if self.hits.load(Ordering::SeqCst) > 0
            && self
                .plan
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|p| p.expected.event_id == id)
        {
            self.raw_reads.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for RawRepo {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(false, event.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(true, event.to_generic()).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for RawRepo {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for RawRepo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for RawRepo {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.inner.server_recovery_response_store()
    }
}
