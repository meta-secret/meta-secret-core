use super::*;
use async_trait::async_trait;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Fault {
    Before,
    Drop,
}
struct Plan {
    owner: UserId,
    uuid: String,
    fault: Fault,
}
pub(super) struct Scrub {
    pub authorization: Option<DeviceTransportNoop>,
    pub prior_auth_readback: bool,
}
#[derive(Clone)]
struct OwnBody {
    original: DeviceLogObject,
    witness_id: ArtifactId,
}
pub(super) struct BodyChange {
    pub authorization: Option<DeviceTransportNoop>,
    pub prior_auth_readback: bool,
    pub deletion: bool,
}
/// Test-only actual backend boundary, never a successful transport stub.
pub(super) struct Repo {
    pub inner: Arc<InMemKvLogEventRepo>,
    plan: Mutex<Option<Plan>>,
    pub hits: AtomicUsize,
    pub attempted_body: Mutex<Option<DeviceLogObject>>,
    pub origin_before_body: Mutex<Option<LocalGuardedPreparationObject>>,
    origin_reads: Mutex<Vec<LocalGuardedPreparationObject>>,
    registry_reads: Mutex<Vec<LocalRemovalEnvelopeObject>>,
    pub auth_reads: Mutex<Vec<DeviceTransportNoop>>,
    pub scrubs: Mutex<Vec<Scrub>>,
    pub packet_writes: Mutex<Vec<SsWorkflowObject>>,
    own_body: Mutex<Option<OwnBody>>,
    pub captured_original_before_auth: AtomicBool,
    pub body_changes: Mutex<Vec<BodyChange>>,
}
impl Repo {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(InMemKvLogEventRepo::default()),
            plan: Mutex::new(None),
            hits: 0.into(),
            attempted_body: Mutex::new(None),
            origin_before_body: Mutex::new(None),
            origin_reads: Mutex::new(vec![]),
            registry_reads: Mutex::new(vec![]),
            auth_reads: Mutex::new(vec![]),
            scrubs: Mutex::new(vec![]),
            packet_writes: Mutex::new(vec![]),
            own_body: Mutex::new(None),
            captured_original_before_auth: false.into(),
            body_changes: Mutex::new(vec![]),
        }
    }
    pub fn arm(&self, owner: UserId, uuid: String, fault: Fault) {
        *self.plan.lock().unwrap() = Some(Plan { owner, uuid, fault });
    }
    pub async fn capture_own_body(
        &self,
        body: DeviceLogObject,
        witness_id: ArtifactId,
    ) -> Result<()> {
        let raw = self
            .inner
            .find_one(witness_id.clone())
            .await?
            .context("actual witness before auth capture")?;
        let witness: LocalGuardedPreparationObject = raw.try_into()?;
        ensure!(
            self.inner.find_one(body.obj_id()).await? == Some(body.clone().to_generic())
                && witness.0.value.transport_noop.is_none()
                && witness
                    .0
                    .value
                    .origin
                    .as_ref()
                    .is_some_and(|o| o.event_id == body.obj_id()
                        && producer_source_commitment(&body).is_ok_and(|h| h == o.body_commitment)),
            "own N capture must read actual ORIGINAL body before any authorization"
        );
        *self.own_body.lock().unwrap() = Some(OwnBody {
            original: body,
            witness_id,
        });
        self.captured_original_before_auth
            .store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn audit_body_change(&self, id: &ArtifactId, replacement: Option<&GenericKvLogEvent>) {
        let selected = self.own_body.lock().unwrap().clone();
        let Some(owned) = selected.filter(|o| o.original.obj_id() == *id) else {
            return;
        };
        if replacement == Some(&owned.original.to_generic()) {
            return;
        }
        let prior_reads = self.auth_reads.lock().unwrap().clone();
        let authorization = match self.inner.find_one(owned.witness_id).await {
            Ok(Some(GenericKvLogEvent::LocalGuardedPreparation(w))) => {
                w.0.value.transport_noop.map(|n| n.authorization)
            }
            _ => None,
        };
        let prior_auth_readback = authorization
            .as_ref()
            .is_some_and(|auth| prior_reads.contains(auth));
        self.body_changes.lock().unwrap().push(BodyChange {
            authorization,
            prior_auth_readback,
            deletion: replacement.is_none(),
        });
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        // Exact physical N, ANY replacement kind/context; no Vault/Pass/cipher filter.
        self.audit_body_change(&event.obj_id(), Some(&event)).await;
        if let GenericKvLogEvent::SsWorkflow(packet @ SsWorkflowObject::Distribution(_)) = &event {
            self.packet_writes.lock().unwrap().push(packet.clone());
        }
        let plan = self
            .plan
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| (p.owner.clone(), p.uuid.clone(), p.fault));
        if let (Some((owner, uuid, fault)), GenericKvLogEvent::DeviceLog(body)) = (plan, &event) {
            let stream = ArtifactId::from(DeviceLogDescriptor::from(owner.clone())).fqdn;
            if body.obj_id().fqdn == stream {
                let id = ArtifactId::from(LocalGuardedPreparationDescriptor {
                    owner: owner.clone(),
                    request_id: uuid.clone(),
                });
                let actual: LocalGuardedPreparationObject = self
                    .inner
                    .find_one(id)
                    .await?
                    .context("actual durable origin before body save")?
                    .try_into()?;
                let registry = PersistentRemovalEnvelope {
                    p_obj: Arc::new(PersistentObject::new(self.inner.clone())),
                }
                .load(&owner)
                .await?
                .context("actual durable registry before body save")?;
                ensure!(
                    actual
                        .0
                        .value
                        .origin
                        .as_ref()
                        .is_some_and(|o| o.event_id == body.obj_id()
                            && o.request_id == uuid
                            && producer_source_commitment(body)
                                .is_ok_and(|h| h == o.body_commitment))
                        && actual
                            .0
                            .value
                            .payload
                            .as_ref()
                            .is_some_and(|p| p.passes.len() == 2)
                        && self.origin_reads.lock().unwrap().contains(&actual)
                        && registry
                            .0
                            .value
                            .entries
                            .iter()
                            .any(|e| e.event_id == body.obj_id()
                                && e.mode == RemovalEnvelopeMode::Guarded
                                && e.request_id.as_deref() == Some(&uuid))
                        && self.registry_reads.lock().unwrap().contains(&registry),
                    "prebody fault requires actual exact origin+registry production readbacks"
                );
                ensure!(
                    self.inner.find_one(body.obj_id()).await?.is_none(),
                    "prebody selected body was already durable"
                );
                *self.attempted_body.lock().unwrap() = Some(body.clone());
                *self.origin_before_body.lock().unwrap() = Some(actual);
                self.hits.fetch_add(1, Ordering::SeqCst);
                if fault == Fault::Before {
                    anyhow::bail!("actual prebody save-before PRIVATE_SENTINEL");
                }
                return Ok(body.obj_id()); // Actual silent Drop; the body was NEVER saved/deleted.
            }
        }
        if let GenericKvLogEvent::LocalGuardedPreparation(candidate) = &event {
            let old = self.inner.find_one(candidate.obj_id()).await?;
            if candidate.0.value.payload.is_none()
                && matches!(old, Some(GenericKvLogEvent::LocalGuardedPreparation(ref p)) if p.0.value.payload.is_some())
            {
                let authorization = candidate
                    .0
                    .value
                    .transport_noop
                    .as_ref()
                    .map(|n| n.authorization.clone());
                let prior_auth_readback = authorization
                    .as_ref()
                    .is_some_and(|auth| self.auth_reads.lock().unwrap().contains(auth));
                self.scrubs.lock().unwrap().push(Scrub {
                    authorization,
                    prior_auth_readback,
                });
            }
        }
        if replace {
            self.inner.replace(event).await
        } else {
            self.inner.save(event).await
        }
    }
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let actual = self.inner.find_one(id).await?;
        match &actual {
            Some(GenericKvLogEvent::LocalGuardedPreparation(p)) => {
                if p.0.value.origin.is_some() {
                    self.origin_reads.lock().unwrap().push(p.clone());
                }
                if let Some(n) = &p.0.value.transport_noop {
                    self.auth_reads
                        .lock()
                        .unwrap()
                        .push(n.authorization.clone());
                }
            }
            Some(GenericKvLogEvent::LocalRemovalEnvelope(r)) => {
                self.registry_reads.lock().unwrap().push(r.clone())
            }
            _ => {}
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
        self.audit_body_change(&id, None).await;
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.audit_body_change(&id, None).await;
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
