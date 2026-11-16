use super::*;
use meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore;

pub(super) struct Outer<R: KvLogEventRepo> {
    pub inner: Arc<ActualRepo<R>>,
    pub physical: Arc<R>,
    pub clock: Arc<AtomicUsize>,
    pub origin: GuardedCommandOrigin,
    witness: ArtifactId,
    mode: Mode,
    targeted: AtomicBool,
    read_fault: AtomicBool,
    pub hits: AtomicUsize,
    pub read_hits: AtomicUsize,
    pub methods: Mutex<Vec<&'static str>>,
    pub writes: Mutex<Vec<Write>>,
    pub reads: Mutex<Vec<Read>>,
    pub durable_auth: Mutex<Vec<DeviceTransportNoop>>,
    pub durable_receipts: Mutex<Vec<DeviceTransportNoopReceipt>>,
    pub changes: Mutex<Vec<BodyChange>>,
}
impl<R: KvLogEventRepo> Outer<R> {
    pub fn new(
        case: &GenericCase<R>,
        physical: Arc<R>,
        origin: GuardedCommandOrigin,
        mode: Mode,
    ) -> Self {
        Self {
            inner: case.h.p.repo.clone(),
            physical,
            clock: Arc::new(0.into()),
            origin,
            witness: case.h.state.witness_id(),
            mode,
            targeted: false.into(),
            read_fault: false.into(),
            hits: 0.into(),
            read_hits: 0.into(),
            methods: Mutex::new(vec![]),
            writes: Mutex::new(vec![]),
            reads: Mutex::new(vec![]),
            durable_auth: Mutex::new(vec![]),
            durable_receipts: Mutex::new(vec![]),
            changes: Mutex::new(vec![]),
        }
    }
    fn call(&self, method: &'static str) {
        self.methods.lock().unwrap().push(method);
    }
    pub fn clear(&self) {
        self.read_fault.store(false, Ordering::SeqCst);
    }
    pub async fn witness(&self) -> Result<LocalGuardedPreparationObject> {
        self.physical
            .find_one(self.witness.clone())
            .await?
            .context("actual physical fixed witness")?
            .try_into()
    }
    pub fn auth_read(&self, auth: &DeviceTransportNoop, before: usize) -> bool {
        self.reads.lock().unwrap().iter().any(|r|r.order<before && matches!(&r.event,
            GenericKvLogEvent::LocalGuardedPreparation(w) if w.0.value.transport_noop.as_ref().is_some_and(|n|&n.authorization==auth)))
    }
    pub fn receipt_read(&self, receipt: &DeviceTransportNoopReceipt, before: usize) -> bool {
        self.reads.lock().unwrap().iter().any(|r|r.order<before && matches!(&r.event,
            GenericKvLogEvent::LocalGuardedPreparation(w) if w.0.value.transport_noop.as_ref().and_then(|n|n.receipt.as_ref())==Some(receipt)))
    }
    async fn body_change(
        &self,
        id: &ArtifactId,
        replacement: Option<&GenericKvLogEvent>,
        order: usize,
    ) -> Result<()> {
        if *id != self.origin.event_id {
            return Ok(());
        }
        let original = self.physical.find_one(id.clone()).await?;
        if original.as_ref() == replacement {
            return Ok(());
        }
        let w = self.witness().await?;
        let ready =
            w.0.value
                .transport_noop
                .as_ref()
                .is_some_and(|n| self.auth_read(&n.authorization, order));
        self.changes.lock().unwrap().push(BodyChange {
            after_auth_readback: ready,
            deletion: replacement.is_none(),
        });
        Ok(())
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        let order = tick(&self.clock);
        self.writes.lock().unwrap().push(Write {
            order,
            event: event.clone(),
        });
        self.body_change(&event.obj_id(), Some(&event), order)
            .await?;
        let old = self.physical.find_one(self.witness.clone()).await?;
        let transition = match (&old, &event) {
            (
                Some(GenericKvLogEvent::LocalGuardedPreparation(old)),
                GenericKvLogEvent::LocalGuardedPreparation(new),
            ) if new.obj_id() == self.witness
                && new.0.value.origin.as_ref() == Some(&self.origin)
                && old.0.value.origin.as_ref() == Some(&self.origin)
                && old.0.value.payload.is_none()
                && new.0.value.payload.is_none() =>
            {
                match (&old.0.value.transport_noop, &new.0.value.transport_noop) {
                    (Some(a), Some(b)) if a.authorization == b.authorization => {
                        if self.mode == Mode::ConfirmedReadback {
                            a.receipt.is_some()
                                && a.receipt == b.receipt
                                && a.confirmed_next_tail.is_none()
                                && b.confirmed_next_tail.is_some()
                        } else {
                            a.receipt.is_none()
                                && b.receipt.is_some()
                                && a.confirmed_next_tail.is_none()
                                && b.confirmed_next_tail.is_none()
                        }
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        let selected =
            self.mode.repo_fault() && transition && !self.targeted.swap(true, Ordering::SeqCst);
        if selected {
            self.hits.fetch_add(1, Ordering::SeqCst);
            match self.mode {
                Mode::ReceiptBefore => anyhow::bail!("RECEIPT_TAIL_PRIVATE_BACKEND"),
                Mode::ReceiptDrop => return Ok(event.obj_id()),
                _ => {}
            }
        }
        let id = if replace {
            self.inner.replace(event.clone()).await?
        } else {
            self.inner.save(event.clone()).await?
        };
        if event.obj_id() == self.witness {
            let actual = self.physical.find_one(self.witness.clone()).await?;
            ensure!(
                actual == Some(event.clone()),
                "actual transport witness write differs"
            );
            if let Some(GenericKvLogEvent::LocalGuardedPreparation(w)) = actual
                && let Some(n) = w.0.value.transport_noop
            {
                self.durable_auth.lock().unwrap().push(n.authorization);
                if let Some(receipt) = n.receipt {
                    self.durable_receipts.lock().unwrap().push(receipt);
                }
            }
        }
        if selected {
            match self.mode {
                Mode::ReceiptReadback | Mode::ConfirmedReadback => {
                    self.read_fault.store(true, Ordering::SeqCst)
                }
                Mode::ReceiptLostReply => anyhow::bail!("RECEIPT_TAIL_PRIVATE_BACKEND"),
                _ => {}
            }
        }
        Ok(id)
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for Outer<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.call("find_one");
        if id == self.witness && self.read_fault.swap(false, Ordering::SeqCst) {
            self.read_hits.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("RECEIPT_TAIL_PRIVATE_BACKEND");
        }
        let result = self.inner.find_one(id).await?;
        if let Some(event) = &result {
            self.reads.lock().unwrap().push(Read {
                order: tick(&self.clock),
                event: event.clone(),
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
        let _ = self.body_change(&id, None, tick(&self.clock)).await;
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.call("try_delete");
        self.body_change(&id, None, tick(&self.clock)).await?;
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
