use super::*;

/// OUTER boundary: internal inherited readbacks are not production acknowledgements.
pub(super) struct Repo {
    pub inner: Arc<ActualRepo>,
    pub physical: Arc<InMemKvLogEventRepo>,
    pub clock: Arc<AtomicUsize>,
    witness_id: ArtifactId,
    origin: GuardedCommandOrigin,
    mode: Mode,
    targeted: AtomicBool,
    block_read: AtomicBool,
    first_read: AtomicBool,
    pub hits: AtomicUsize,
    pub read_hits: AtomicUsize,
    pub intended: Mutex<Option<DeviceTransportNoop>>,
    pub writes: Mutex<Vec<Write>>,
    pub reads: Mutex<Vec<Read>>,
    pub durable: Mutex<Vec<Durable>>,
}
impl Repo {
    pub fn new(
        case: &Case,
        local: &Arc<local_repo::Repo>,
        origin: GuardedCommandOrigin,
        mode: Mode,
    ) -> Self {
        Self {
            inner: case.h.p.repo.clone(),
            physical: local.inner.clone(),
            clock: Arc::new(0.into()),
            witness_id: case.h.state.witness_id(),
            origin,
            mode,
            targeted: false.into(),
            block_read: false.into(),
            first_read: false.into(),
            hits: 0.into(),
            read_hits: 0.into(),
            intended: Mutex::new(None),
            writes: Mutex::new(vec![]),
            reads: Mutex::new(vec![]),
            durable: Mutex::new(vec![]),
        }
    }
    pub fn clear(&self) {
        self.block_read.store(false, Ordering::SeqCst);
    }
    pub async fn witness(&self) -> Result<LocalGuardedPreparationObject> {
        self.physical
            .find_one(self.witness_id.clone())
            .await?
            .context("actual physical witness")?
            .try_into()
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        self.writes.lock().unwrap().push(Write {
            order: tick(&self.clock),
            event: event.clone(),
        });
        let old = self.physical.find_one(self.witness_id.clone()).await?;
        let first_auth = match (&event, &old) {
            (
                GenericKvLogEvent::LocalGuardedPreparation(candidate),
                Some(GenericKvLogEvent::LocalGuardedPreparation(old)),
            ) => {
                candidate.obj_id() == self.witness_id
                    && candidate.0.value.origin.as_ref() == Some(&self.origin)
                    && old.0.value.origin.as_ref() == Some(&self.origin)
                    && old.0.value.payload.is_some()
                    && old.0.value.transport_noop.is_none()
                    && candidate.0.value.transport_noop.is_some()
            }
            _ => false,
        };
        let selected = first_auth && !self.targeted.swap(true, Ordering::SeqCst);
        if selected {
            let GenericKvLogEvent::LocalGuardedPreparation(candidate) = &event else {
                unreachable!()
            };
            *self.intended.lock().unwrap() = candidate
                .0
                .value
                .transport_noop
                .as_ref()
                .map(|n| n.authorization.clone());
            self.hits.fetch_add(1, Ordering::SeqCst);
            match self.mode {
                Mode::Before => anyhow::bail!("AUTH4_PRIVATE_BACKEND"),
                Mode::Drop => return Ok(event.obj_id()),
                _ => {}
            }
        }
        let id = if replace {
            self.inner.replace(event.clone()).await?
        } else {
            self.inner.save(event.clone()).await?
        };
        if event.obj_id() == self.witness_id {
            let actual = self.physical.find_one(self.witness_id.clone()).await?;
            ensure!(
                actual == Some(event),
                "actual auth/private write did not persist exact bytes"
            );
            if let Some(GenericKvLogEvent::LocalGuardedPreparation(w)) = actual
                && let Some(n) = w.0.value.transport_noop
            {
                self.durable.lock().unwrap().push(Durable {
                    order: tick(&self.clock),
                    authorization: n.authorization,
                });
            }
        }
        if selected {
            match self.mode {
                Mode::ReadbackErr => {
                    self.block_read.store(true, Ordering::SeqCst);
                    self.first_read.store(true, Ordering::SeqCst);
                }
                Mode::LostReply => anyhow::bail!("AUTH4_PRIVATE_BACKEND"),
                _ => {}
            }
        }
        Ok(id)
    }
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        if id == self.witness_id && self.block_read.load(Ordering::SeqCst) {
            let physical = self.physical.find_one(id.clone()).await?;
            let full_auth = matches!(physical, Some(GenericKvLogEvent::LocalGuardedPreparation(ref w))
                if w.0.value.payload.is_some() && w.0.value.transport_noop.is_some());
            if self.first_read.swap(false, Ordering::SeqCst) || full_auth {
                self.read_hits.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("AUTH4_PRIVATE_BACKEND"); // BEFORE inherited successful-read observers.
            }
        }
        let actual = self.inner.find_one(id).await?;
        if let Some(GenericKvLogEvent::LocalGuardedPreparation(w)) = &actual
            && let Some(n) = &w.0.value.transport_noop
        {
            self.reads.lock().unwrap().push(Read {
                order: tick(&self.clock),
                authorization: n.authorization.clone(),
            });
        }
        Ok(actual)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(false, event.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(true, event.to_generic()).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Repo {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Repo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for Repo {}
