use super::*;
struct Plan {
    cut: Cut,
    witness: ArtifactId,
    index: ArtifactId,
    input: ArtifactId,
    next: ArtifactId,
    old: LocalGuardedPreparationObject,
    old_index: LocalGuardedPreparationIndexObject,
    revision: u64,
}
/// Outermost production adapter; raw physical probes never enter its success trace.
pub(super) struct Gate<R: KvLogEventRepo> {
    pub inner: Arc<OldRepo<R>>,
    pub physical: Arc<R>,
    clock: AtomicUsize,
    plan: Mutex<Option<Plan>>,
    read_fault: Mutex<Option<ArtifactId>>,
    selected: AtomicBool,
    pub hits: AtomicUsize,
    pub read_hits: AtomicUsize,
    pub calls: Mutex<Vec<Call>>,
    pub success: Mutex<Vec<Success>>,
}
impl<R: KvLogEventRepo> Gate<R> {
    pub fn new(h: &OldHarness<R>) -> Self {
        Self {
            inner: h.repo.clone(),
            physical: h.physical.clone(),
            clock: 0.into(),
            plan: Mutex::new(None),
            read_fault: Mutex::new(None),
            selected: false.into(),
            hits: 0.into(),
            read_hits: 0.into(),
            calls: Mutex::new(vec![]),
            success: Mutex::new(vec![]),
        }
    }
    fn call(&self, operation: Operation) -> usize {
        let order = self.clock.fetch_add(1, Ordering::SeqCst) + 1;
        self.calls.lock().unwrap().push(Call { order, operation });
        order
    }
    pub fn reset(&self) {
        self.calls.lock().unwrap().clear();
        self.success.lock().unwrap().clear();
    }
    pub fn clear(&self) {
        *self.plan.lock().unwrap() = None;
        *self.read_fault.lock().unwrap() = None;
    }
    pub fn arm(&self, cut: Cut, h: &OldHarness<R>, old: &Retired) {
        self.reset();
        self.selected.store(false, Ordering::SeqCst);
        self.hits.store(0, Ordering::SeqCst);
        self.read_hits.store(0, Ordering::SeqCst);
        *self.plan.lock().unwrap() = Some(Plan {
            cut,
            witness: h.case.h.state.witness_id(),
            index: h.case.h.state.index_id(),
            input: ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: h.case.h.state.passes[1].clone(),
                receiver: h.case.h.state.b.device_id().clone(),
            })),
            next: old
                .witness
                .0
                .value
                .origin
                .as_ref()
                .unwrap()
                .event_id
                .clone()
                .next(),
            old: old.witness.clone(),
            old_index: old.index.clone(),
            revision: old.index.0.value.revision,
        });
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        self.call(Operation::Write(replace, Box::new(event.clone())));
        let targeted = {
            let plan = self.plan.lock().unwrap();
            if let Some(p) = plan.as_ref() {
                let full = replace
                    && matches!(&event,GenericKvLogEvent::LocalGuardedPreparation(w)
                    if w.obj_id()==p.witness && w.0.value.payload.is_some() && w.0.value.origin.is_none()
                    && w.0.value.previous_attempts.len()==p.old.0.value.previous_attempts.len()+1);
                let origin = replace
                    && matches!(&event,GenericKvLogEvent::LocalGuardedPreparation(w)
                    if w.obj_id()==p.witness && w.0.value.payload.is_some() && w.0.value.phase==GuardedPreparationPhase::Staged
                    && w.0.value.origin.as_ref().is_some_and(|o|o.event_id==p.next));
                let index = replace
                    && matches!(&event,GenericKvLogEvent::LocalGuardedPreparationIndex(x)
                    if x.obj_id()==p.index && x.0.value.revision==p.revision+1);
                let body = !replace
                    && matches!(&event,GenericKvLogEvent::DeviceLog(b) if b.obj_id()==p.next);
                let matches = match p.cut {
                    Cut::IndexDrop => index,
                    Cut::FullBefore | Cut::FullDrop | Cut::FullReadback | Cut::FullLostReply => {
                        full
                    }
                    Cut::OriginReadback => origin,
                    Cut::BodyBefore | Cut::BodyReadback => body,
                    Cut::ComputeRead => false,
                };
                if matches && !self.selected.swap(true, Ordering::SeqCst) {
                    Some(p.cut)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(cut) = targeted {
            let (old, index, witness, next) = {
                let plan = self.plan.lock().unwrap();
                let p = plan.as_ref().unwrap();
                (
                    p.old.clone(),
                    p.old_index.clone(),
                    p.witness.clone(),
                    p.next.clone(),
                )
            };
            match cut {
                Cut::IndexDrop => ensure!(
                    self.physical.find_one(index.obj_id()).await? == Some(index.to_generic()),
                    "changed index cut lacks exact existing old index"
                ),
                Cut::FullBefore | Cut::FullDrop | Cut::FullReadback | Cut::FullLostReply => {
                    ensure!(
                        self.physical.find_one(witness).await? == Some(old.to_generic()),
                        "first FULL fault did not target actual old payloadNone witness"
                    )
                }
                Cut::OriginReadback => {
                    let before: LocalGuardedPreparationObject = self
                        .physical
                        .find_one(witness)
                        .await?
                        .context("real before-origin FULL")?
                        .try_into()?;
                    ensure!(
                        before.0.value.payload.is_some() && before.0.value.origin.is_none(),
                        "origin cut did not target real changed None→Some"
                    );
                }
                Cut::BodyBefore | Cut::BodyReadback => {
                    let before: LocalGuardedPreparationObject = self
                        .physical
                        .find_one(witness)
                        .await?
                        .context("real before-body origin")?
                        .try_into()?;
                    ensure!(
                        self.physical.find_one(next.clone()).await?.is_none()
                            && before
                                .0
                                .value
                                .origin
                                .as_ref()
                                .is_some_and(|o| o.event_id == next),
                        "body cut did not target actual origin-attached/body-absent N+1"
                    );
                }
                _ => {}
            }
            self.hits.fetch_add(1, Ordering::SeqCst);
            match cut {
                Cut::FullBefore | Cut::BodyBefore => anyhow::bail!("ROLLOVER_PRIVATE_BACKEND"),
                Cut::IndexDrop | Cut::FullDrop => return Ok(event.obj_id()),
                _ => {}
            }
        }
        let id = if replace {
            self.inner.replace(event.clone()).await?
        } else {
            self.inner.save(event.clone()).await?
        };
        if let Some(cut) = targeted {
            ensure!(
                self.physical.find_one(event.obj_id()).await? == Some(event.clone()),
                "real changed operation did not durably commit"
            );
            match cut {
                Cut::FullReadback | Cut::OriginReadback | Cut::BodyReadback => {
                    *self.read_fault.lock().unwrap() = Some(event.obj_id())
                }
                Cut::FullLostReply => anyhow::bail!("ROLLOVER_PRIVATE_BACKEND"),
                _ => {}
            }
        }
        Ok(id)
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for Gate<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.call(Operation::Read(id.clone()));
        let compute = {
            let plan = self.plan.lock().unwrap();
            plan.as_ref()
                .is_some_and(|p| p.cut == Cut::ComputeRead && id == p.input)
        };
        if compute && !self.selected.swap(true, Ordering::SeqCst) {
            let (old, index) = {
                let p = self.plan.lock().unwrap();
                let p = p.as_ref().unwrap();
                (p.old.clone(), p.old_index.clone())
            };
            let reads = self.success.lock().unwrap();
            ensure!(
                reads.iter().any(|r| r.event == old.clone().to_generic())
                    && reads.iter().any(|r| r.event == index.clone().to_generic())
                    && self.calls.lock().unwrap().iter().all(|c| !matches!(
                        &c.operation,
                        Operation::Write(_, event) if matches!(
                            event.as_ref(),
                            GenericKvLogEvent::LocalGuardedPreparationIndex(_)
                                | GenericKvLogEvent::LocalGuardedPreparation(_)
                        )
                    )),
                "compute cut not after actual old capture/before probe"
            );
            self.hits.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("ROLLOVER_PRIVATE_BACKEND");
        }
        let fail = {
            let mut fault = self.read_fault.lock().unwrap();
            if fault.as_ref() == Some(&id) {
                fault.take();
                true
            } else {
                false
            }
        };
        if fail {
            self.read_hits.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("ROLLOVER_PRIVATE_BACKEND");
        }
        let result = self.inner.find_one(id).await?;
        if let Some(event) = &result {
            self.success.lock().unwrap().push(Success {
                order: self.clock.fetch_add(1, Ordering::SeqCst) + 1,
                event: event.clone(),
            });
        }
        Ok(result)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.call(Operation::Key(id.clone()));
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for Gate<R> {
    async fn save<T: ToGenericEvent>(&self, e: T) -> Result<ArtifactId> {
        self.write(false, e.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, e: T) -> Result<ArtifactId> {
        self.write(true, e.to_generic()).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for Gate<R> {
    async fn delete(&self, id: ArtifactId) {
        self.call(Operation::Delete(id.clone()));
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.call(Operation::TryDelete(id.clone()));
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for Gate<R> {
    async fn db_clean_up(&self) {
        self.call(Operation::Clean);
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for Gate<R> {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.call(Operation::Recovery);
        self.inner.server_recovery_response_store()
    }
}
