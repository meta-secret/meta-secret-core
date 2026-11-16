use super::*;
use async_trait::async_trait;

/// Forwarding actual backend, disabled until the checked producer has completed.
pub(super) struct Observed<R: KvLogEventRepo> {
    pub inner: Arc<R>,
    pub observations: Arc<Observations>,
}
impl<R: KvLogEventRepo> Observed<R> {
    pub fn new(inner: Arc<R>, observations: Arc<Observations>) -> Self {
        Self {
            inner,
            observations,
        }
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        let expected = self.observations.expected.lock().unwrap().clone();
        let owner = self.observations.owner.lock().unwrap().clone();
        let relevant = match (&expected, &event) {
            (_, GenericKvLogEvent::SsWorkflow(packet @ SsWorkflowObject::Distribution(x))) => {
                owner
                    .as_ref()
                    .is_some_and(|owner| x.value.vault_name == owner.vault_name)
                    || expected.as_ref().is_some_and(|e| current_packet(e, packet))
            }
            _ => false,
        };
        if owner
            .as_ref()
            .is_some_and(|owner| current_claim_record(owner, &event))
        {
            self.observations
                .claim_writes
                .lock()
                .unwrap()
                .push(event.clone());
        }
        let observation = if relevant {
            let local_applied =
                if let (Some(e), Some(action)) = (expected.as_ref(), self.observations.action()) {
                    proof::applied(self.inner.clone(), e, &action)
                        .await
                        .unwrap_or(false)
                } else {
                    false
                };
            let promoting_readback = if let Some(e) = expected.as_ref() {
                self.inner
                    .find_one(e.witness_id.clone())
                    .await?
                    .and_then(|x| LocalGuardedPreparationObject::try_from(x).ok())
                    .is_some_and(|x| {
                        x.0.value.phase == GuardedPreparationPhase::AcceptedPromoting
                            && x.0.value.payload.as_ref() == Some(&e.frozen)
                    })
            } else {
                false
            };
            let prior_exact = self.inner.find_one(event.obj_id()).await? == Some(event.clone());
            let mut live = self.observations.live.lock().unwrap();
            let index = live.len();
            live.push(LiveWrite {
                packet: event.clone().shared_secret()?,
                local_applied,
                promoting_readback,
                prior_exact,
                durable: false,
            });
            Some(index)
        } else {
            None
        };
        // Match actual intended NEW generation/Pass/recipient, not cipher equality:
        // a changed cipher in that generation must still reach the fault boundary.
        let intended_generation = expected.as_ref().is_some_and(|e| {
            event
                .clone()
                .shared_secret()
                .is_ok_and(|packet| proof::intended(e, &packet))
        });
        if intended_generation
            && observation.is_some()
            && self.observations.live.lock().unwrap()[observation.unwrap()].local_applied
        {
            let count = self
                .observations
                .post_applied_writes
                .fetch_add(1, Ordering::SeqCst)
                + 1;
            if self.observations.fail_second.load(Ordering::SeqCst) && count == 2 {
                self.observations.fault_hits.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("C2 second actual live publication unavailable");
            }
        }
        let id = if replace {
            self.inner.replace(event.clone()).await?
        } else {
            self.inner.save(event.clone()).await?
        };
        let exact = self.inner.find_one(id.clone()).await? == Some(event.clone());
        if let Some(index) = observation {
            self.observations.live.lock().unwrap()[index].durable = exact;
        }
        if let (Some(e), GenericKvLogEvent::SsDeviceLog(claim)) = (&expected, &event)
            && exact
            && e.frozen.passes.iter().any(|p| p.claim == claim.0.value)
        {
            self.observations.claims.lock().unwrap().push(claim.clone());
        }
        Ok(id)
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for Observed<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for Observed<R> {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(false, event.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(true, event.to_generic()).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for Observed<R> {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for Observed<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for Observed<R> {}
