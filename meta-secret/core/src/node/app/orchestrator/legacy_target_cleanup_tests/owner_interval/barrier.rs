use super::*;
use crate::node::db::repo::recovery_response_store::ServerRecoveryResponseStore;

#[derive(Debug)]
pub(super) struct TransportBoundary;
impl std::fmt::Display for TransportBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("owner interval first transport boundary")
    }
}
impl std::error::Error for TransportBoundary {}

pub(super) struct Outer {
    pub inner: Arc<repo::Observed>,
    old_target: SsWorkflowObject,
    delete_release: flume::Receiver<()>,
    pub delete_entered: AtomicBool,
    pub old_at_entry: AtomicBool,
    pub methods: Mutex<Vec<&'static str>>,
    pub writes: Mutex<Vec<GenericKvLogEvent>>,
    pub delete_keys: Mutex<Vec<ArtifactId>>,
}
impl Outer {
    pub fn new(h: &fixture::Harness, delete_release: flume::Receiver<()>) -> Self {
        Self {
            inner: h.world.repo.clone(),
            old_target: h.old_target.clone(),
            delete_release,
            delete_entered: false.into(),
            old_at_entry: false.into(),
            methods: Mutex::new(vec![]),
            writes: Mutex::new(vec![]),
            delete_keys: Mutex::new(vec![]),
        }
    }
    fn call(&self, method: &'static str) {
        self.methods.lock().unwrap().push(method);
    }
    pub fn calls(&self) -> usize {
        self.methods.lock().unwrap().len()
    }
    pub fn material_attempts(&self) -> usize {
        self.writes
            .lock()
            .unwrap()
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
impl FindOneQuery for Outer {
    async fn find_one(&self, key: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.call("find_one");
        self.inner.find_one(key).await
    }
    async fn get_key(&self, key: ArtifactId) -> Result<Option<ArtifactId>> {
        self.call("get_key");
        self.inner.get_key(key).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Outer {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.call("save");
        let event = event.to_generic();
        self.writes.lock().unwrap().push(event.clone());
        self.inner.save(event).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.call("replace");
        let event = event.to_generic();
        self.writes.lock().unwrap().push(event.clone());
        self.inner.replace(event).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Outer {
    async fn delete(&self, key: ArtifactId) {
        self.call("delete");
        self.delete_keys.lock().unwrap().push(key.clone());
        self.inner.delete(key).await
    }
    async fn try_delete(&self, key: ArtifactId) -> Result<()> {
        self.call("try_delete");
        self.delete_keys.lock().unwrap().push(key.clone());
        if key == self.old_target.obj_id() && !self.delete_entered.load(Ordering::SeqCst) {
            // Physical inspection is never a production-read acknowledgement.
            let unchanged = self.inner.inner.find_one(key.clone()).await?
                == Some(self.old_target.clone().to_generic());
            self.old_at_entry.store(unchanged, Ordering::SeqCst);
            ensure!(
                unchanged,
                "livepoint must precede forwarding exact old target deletion"
            );
            self.delete_entered.store(true, Ordering::SeqCst);
            self.delete_release
                .recv_async()
                .await
                .context("delete release channel")?;
        }
        self.inner.try_delete(key).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Outer {
    async fn db_clean_up(&self) {
        self.call("db_clean_up");
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for Outer {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.call("server_recovery_response_store");
        self.inner.server_recovery_response_store()
    }
}

pub(super) struct Wire {
    pub first_entered: AtomicBool,
    first_release: flume::Receiver<()>,
    owner: UserData,
    pub attempts: AtomicUsize,
    pub after_legacy: Mutex<Vec<bool>>,
    pub legacy_done: AtomicBool,
}
impl Wire {
    pub fn new(owner: UserData, first_release: flume::Receiver<()>) -> Self {
        Self {
            first_entered: false.into(),
            first_release,
            owner,
            attempts: 0.into(),
            after_legacy: Mutex::new(vec![]),
            legacy_done: false.into(),
        }
    }
}
impl SyncProtocol for Wire {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let call = self.attempts.fetch_add(1, Ordering::SeqCst);
        self.after_legacy
            .lock()
            .unwrap()
            .push(self.legacy_done.load(Ordering::SeqCst));
        ensure!(
            matches!(&request,SyncRequest::Read(read) if matches!(&**read,
            ReadSyncRequest::ServerTail(tail) if tail.sender==self.owner)),
            "public sync did not reach its actual first ServerTail request"
        );
        if call == 0 {
            self.first_entered.store(true, Ordering::SeqCst);
            self.first_release
                .recv_async()
                .await
                .context("sync release channel")?;
        }
        // Deliberate typed component cut, never fake Data/ACK or server success.
        Err(anyhow::Error::new(TransportBoundary))
    }
}
