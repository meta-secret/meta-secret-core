use super::*;
use async_trait::async_trait;

#[derive(Clone)]
struct Plan {
    e: Expected,
    fault: Fault,
    acks: Arc<Acks>,
    observations: Arc<Observations>,
}
pub(super) struct PeerRepo<R: KvLogEventRepo> {
    pub inner: Arc<R>,
    plan: Mutex<Option<Plan>>,
    pub armed: AtomicBool,
    pub hits: AtomicUsize,
    pub packet_write_attempts: Mutex<Vec<SsWorkflowObject>>,
    pub forwarded_deletes: Mutex<HashSet<ArtifactId>>,
    pub blocked_id: Mutex<Option<ArtifactId>>,
    last_witness_write: Mutex<Option<LocalGuardedPreparationObject>>,
    pub handed_readbacks: Mutex<HashSet<ArtifactId>>,
    dropped_full: Mutex<Option<LocalGuardedPreparationObject>>,
    pub drop_readbacks: AtomicUsize,
}
impl<R: KvLogEventRepo> PeerRepo<R> {
    pub fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            plan: Mutex::new(None),
            armed: false.into(),
            hits: 0.into(),
            packet_write_attempts: Mutex::new(Vec::new()),
            forwarded_deletes: Mutex::new(HashSet::new()),
            blocked_id: Mutex::new(None),
            last_witness_write: Mutex::new(None),
            handed_readbacks: Mutex::new(HashSet::new()),
            dropped_full: Mutex::new(None),
            drop_readbacks: 0.into(),
        }
    }
    pub fn arm(&self, e: Expected, fault: Fault, acks: Arc<Acks>, observations: Arc<Observations>) {
        *self.plan.lock().unwrap() = Some(Plan {
            e,
            fault,
            acks,
            observations,
        });
        self.armed.store(true, Ordering::SeqCst);
    }
    async fn ready(&self, p: &Plan) -> Result<LocalGuardedPreparationObject> {
        let action = p
            .observations
            .action()
            .context("actual source before peer cleanup")?;
        ensure!(
            proof::applied(self.inner.clone(), &p.e, &action).await?,
            "actual LOCALApplied required before peer fault"
        );
        let w: LocalGuardedPreparationObject = self
            .inner
            .find_one(p.e.witness_id.clone())
            .await?
            .context("actual current peer witness")?
            .try_into()?;
        ensure!(
            w.0.value.phase == GuardedPreparationPhase::AcceptedPromoting
                && w.0.value.payload.as_ref() == Some(&p.e.frozen)
                && w.0.value.packets.iter().all(|p| p.handed_off)
                && p.acks.exact(&p.e).len() == 2,
            "fault requires FULL stage and both real distinct peer ACK/handedOff proofs"
        );
        checks::ordinary(self.inner.clone(), &p.e, &w).await?;
        Ok(w)
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        // Capture every Distribution save/replace attempt before applying ANY
        // Vault/Pass/generation/cipher filter in the negative oracle.
        if let GenericKvLogEvent::SsWorkflow(packet @ SsWorkflowObject::Distribution(_)) = &event {
            self.packet_write_attempts
                .lock()
                .unwrap()
                .push(packet.clone());
        }
        let plan = self.plan.lock().unwrap().clone();
        if let (Some(p), GenericKvLogEvent::LocalGuardedPreparation(candidate)) = (&plan, &event)
            && self.armed.load(Ordering::SeqCst)
            && p.fault == Fault::ScrubDrop
            && replace
            && candidate.obj_id() == p.e.witness_id
            && candidate.0.value.phase == GuardedPreparationPhase::AcceptedPublished
            && candidate.0.value.payload.is_none()
        {
            let old = self.ready(p).await?;
            ensure!(
                checks::local_peers(self.inner.clone(), &p.e)
                    .await?
                    .is_empty(),
                "final scrub preceded peer-copy absence"
            );
            let mut expected = old.clone();
            expected.0.value.payload = None;
            expected.0.value.phase = GuardedPreparationPhase::AcceptedPublished;
            ensure!(
                *candidate == expected,
                "final scrub candidate changed extra immutable metadata"
            );
            *self.dropped_full.lock().unwrap() = Some(old);
            self.hits.fetch_add(1, Ordering::SeqCst);
            return Ok(candidate.obj_id()); // Genuine silent Drop: actual backend still holds FULL.
        }
        let witness = match &event {
            GenericKvLogEvent::LocalGuardedPreparation(x) => Some(x.clone()),
            _ => None,
        };
        let id = if replace {
            self.inner.replace(event).await?
        } else {
            self.inner.save(event).await?
        };
        if let Some(w) = witness {
            *self.last_witness_write.lock().unwrap() = Some(w);
        }
        Ok(id)
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for PeerRepo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let actual = self.inner.find_one(id.clone()).await?;
        let plan = self.plan.lock().unwrap().clone();
        if let Some(p) = plan
            && id == p.e.witness_id
        {
            let intended = self.last_witness_write.lock().unwrap().clone();
            if let Some(w) = intended
                && actual == Some(w.clone().to_generic())
            {
                self.handed_readbacks.lock().unwrap().extend(
                    w.0.value
                        .packets
                        .iter()
                        .filter(|x| x.handed_off && x.receiver_id == p.e.requester)
                        .map(|x| x.event_id.clone()),
                );
            }
            let dropped = self.dropped_full.lock().unwrap().clone();
            if self.armed.load(Ordering::SeqCst)
                && dropped.is_some_and(|w| actual == Some(w.to_generic()))
            {
                self.drop_readbacks.fetch_add(1, Ordering::SeqCst);
            }
        }
        Ok(actual)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for PeerRepo<R> {
    async fn save<T: ToGenericEvent>(&self, x: T) -> Result<ArtifactId> {
        self.write(false, x.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, x: T) -> Result<ArtifactId> {
        self.write(true, x.to_generic()).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for PeerRepo<R> {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        let plan = self.plan.lock().unwrap().clone();
        if let Some(p) = plan
            && peer_packets(&p.e).iter().any(|x| x.obj_id() == id)
        {
            if self.armed.load(Ordering::SeqCst)
                && p.fault == Fault::LastDelete
                && p.acks.exact(&p.e).len() == 2
                && self.forwarded_deletes.lock().unwrap().len() == 1
            {
                let w = self.ready(&p).await?;
                ensure!(
                    self.handed_readbacks.lock().unwrap().contains(&id)
                        && w.0
                            .value
                            .packets
                            .iter()
                            .any(|x| x.event_id == id && x.handed_off),
                    "last peer handoff lacked actual fixed-record readback"
                );
                let raw = self
                    .inner
                    .find_one(id.clone())
                    .await?
                    .context("last actual local peer copy")?;
                ensure!(
                    peer_packets(&p.e)
                        .iter()
                        .any(|x| x.clone().to_generic() == raw),
                    "last actual peer cipher changed"
                );
                *self.blocked_id.lock().unwrap() = Some(id);
                self.hits.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("actual last peer try_delete before-delete PRIVATE_SENTINEL");
            }
            self.inner.try_delete(id.clone()).await?;
            ensure!(
                self.inner.find_one(id.clone()).await?.is_none(),
                "actual forwarded peer delete did not remove copy"
            );
            self.forwarded_deletes.lock().unwrap().insert(id);
            return Ok(());
        }
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for PeerRepo<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for PeerRepo<R> {}
