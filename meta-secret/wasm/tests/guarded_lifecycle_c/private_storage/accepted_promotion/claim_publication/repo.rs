use super::*;
use async_trait::async_trait;

#[derive(Clone)]
struct Plan {
    expected: Expected,
    fault: Fault,
    observations: Arc<Observations>,
}
#[derive(Clone)]
pub(super) struct Attempt {
    pub order: usize,
    pub event: Box<GenericKvLogEvent>,
    pub replace: bool,
    pub returned: usize,
    pub saved: bool,
}
#[derive(Clone)]
pub(super) struct Read {
    pub order: usize,
    pub returned: usize,
    pub id: ArtifactId,
    pub actual: Option<Box<GenericKvLogEvent>>,
    pub success: bool,
}
#[derive(Clone)]
pub(super) struct Rejected {
    pub order: usize,
    pub event: Box<GenericKvLogEvent>,
    pub prior: Option<Box<GenericKvLogEvent>>,
}

/// Backend fault below ALL-writes observer. Only the first intended NEW Split is selected.
pub(super) struct ClaimRepo<R: KvLogEventRepo> {
    pub inner: Arc<R>,
    plan: Mutex<Option<Plan>>,
    pub armed: AtomicBool,
    pub device: Mutex<Option<SsDeviceLogObject>>,
    pub log: Mutex<Option<SsLogObject>>,
    pub device_writes: AtomicUsize,
    pub log_writes: AtomicUsize,
    pub device_reads: AtomicUsize,
    pub first_device_read_exact: AtomicBool,
    pub device_reads_before_log: AtomicUsize,
    pub log_reads: AtomicUsize,
    pub observer_log_forwarded: AtomicUsize,
    pub publisher_read_hits: AtomicUsize,
    pub fault_hits: AtomicUsize,
    pub terminal: Mutex<Option<Terminal>>,
    pub terminal_writes: Mutex<Vec<(SsLogObject, bool)>>,
    clock: AtomicUsize,
    pub attempts: Mutex<Vec<Attempt>>,
    pub reads: Mutex<Vec<Read>>,
    pub rejected: Mutex<Option<Rejected>>,
}
impl<R: KvLogEventRepo> ClaimRepo<R> {
    pub fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            plan: Mutex::new(None),
            armed: false.into(),
            device: Mutex::new(None),
            log: Mutex::new(None),
            device_writes: 0.into(),
            log_writes: 0.into(),
            device_reads: 0.into(),
            first_device_read_exact: false.into(),
            device_reads_before_log: 0.into(),
            log_reads: 0.into(),
            observer_log_forwarded: 0.into(),
            publisher_read_hits: 0.into(),
            fault_hits: 0.into(),
            terminal: Mutex::new(None),
            terminal_writes: Mutex::new(vec![]),
            clock: 0.into(),
            attempts: Mutex::new(vec![]),
            reads: Mutex::new(vec![]),
            rejected: Mutex::new(None),
        }
    }
    pub fn arm(&self, expected: Expected, fault: Fault, observations: Arc<Observations>) {
        *self.plan.lock().unwrap() = Some(Plan {
            expected,
            fault,
            observations,
        });
        self.armed.store(true, Ordering::SeqCst);
    }
    fn tick(&self) -> usize {
        self.clock.fetch_add(1, Ordering::SeqCst) + 1
    }
    fn before_case(&self) -> bool {
        self.plan
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|p| matches!(p.fault, Fault::DeviceBefore | Fault::LogBefore))
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        // Capture ALL attempted writes before any event/Pass/generation selection.
        // Existing fault modes keep their original forwarding/counter semantics.
        let attempt = if self.before_case() {
            let mut attempts = self.attempts.lock().unwrap();
            let index = attempts.len();
            attempts.push(Attempt {
                order: self.tick(),
                event: Box::new(event.clone()),
                replace,
                returned: 0,
                saved: false,
            });
            Some(index)
        } else {
            None
        };
        if let (Some(terminal), GenericKvLogEvent::SsLog(log)) =
            (self.terminal.lock().unwrap().clone(), &event)
        {
            self.terminal_writes
                .lock()
                .unwrap()
                .push((log.clone(), terminal.preserved(&log.0.value)));
        }
        let plan = self.plan.lock().unwrap().clone();
        let (mut device, mut log) = (None, None);
        if let Some(p) = &plan {
            let intended = &p.expected.frozen.passes[0].claim;
            let same = |c: &SsClaim| {
                c.id == intended.id
                    && c.dist_claim_id == intended.dist_claim_id
                    && c.sender == p.expected.owner.device_id
                    && c.distribution_type == SecretDistributionType::Split
            };
            match &event {
                GenericKvLogEvent::SsDeviceLog(x) if same(&x.0.value) => device = Some(x.clone()),
                GenericKvLogEvent::SsLog(x)
                    if x.0.value.claims.get(&intended.id).is_some_and(same)
                        && self
                            .log
                            .lock()
                            .unwrap()
                            .as_ref()
                            .is_none_or(|first| first.obj_id() == x.obj_id()) =>
                {
                    log = Some(x.clone())
                }
                _ => {}
            }
            if device.is_some() || log.is_some() {
                let action = p
                    .observations
                    .action()
                    .context("actual signed source before claim write")?;
                ensure!(
                    proof::applied(self.inner.clone(), &p.expected, &action).await?,
                    "intended NEW claim write preceded actual LOCALApplied"
                );
            }
        }
        if log.is_some() && self.log.lock().unwrap().is_none() {
            self.device_reads_before_log
                .store(self.device_reads.load(Ordering::SeqCst), Ordering::SeqCst);
        }
        if self.armed.load(Ordering::SeqCst)
            && plan.as_ref().is_some_and(|p| {
                (p.fault == Fault::DeviceBefore && device.is_some())
                    || (p.fault == Fault::LogBefore && log.is_some())
            })
        {
            // This physical probe is deliberately NOT an outer production Find.
            let prior = self.inner.find_one(event.obj_id()).await?.map(Box::new);
            let order = self.tick();
            *self.rejected.lock().unwrap() = Some(Rejected {
                order,
                event: Box::new(event),
                prior,
            });
            self.fault_hits.fetch_add(1, Ordering::SeqCst);
            if let Some(index) = attempt {
                self.attempts.lock().unwrap()[index].returned = order;
            }
            anyhow::bail!("actual claim BEFORE-save rejection PRIVATE_SENTINEL");
        }
        let outcome = if replace {
            self.inner.replace(event).await
        } else {
            self.inner.save(event).await
        };
        if let Some(index) = attempt {
            let returned = self.tick();
            let mut attempts = self.attempts.lock().unwrap();
            attempts[index].returned = returned;
            attempts[index].saved = outcome.is_ok();
        }
        let saved = outcome?;
        if let Some(x) = device {
            self.device_writes.fetch_add(1, Ordering::SeqCst);
            *self.device.lock().unwrap() = Some(x);
            if self.armed.load(Ordering::SeqCst)
                && plan
                    .as_ref()
                    .is_some_and(|p| p.fault == Fault::DeviceLostReply)
            {
                self.fault_hits.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("actual durable SsDeviceLog lost reply PRIVATE_SENTINEL");
            }
        }
        if let Some(x) = log {
            self.log_writes.fetch_add(1, Ordering::SeqCst);
            // Retain the FIRST actual allocated SsLog, not later legitimate supersets.
            let mut actual = self.log.lock().unwrap();
            if actual.is_none() {
                *actual = Some(x);
            }
        }
        Ok(saved)
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for ClaimRepo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        if self.before_case() {
            let order = self.tick();
            let outcome = self.inner.find_one(id.clone()).await;
            let returned = self.tick();
            self.reads.lock().unwrap().push(Read {
                order,
                returned,
                id,
                actual: outcome.as_ref().ok().and_then(Clone::clone).map(Box::new),
                success: outcome.is_ok(),
            });
            return outcome;
        }
        let device = self
            .device
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|x| x.obj_id() == id);
        if device {
            let count = self.device_reads.fetch_add(1, Ordering::SeqCst) + 1;
            if count == 1 {
                let actual = self.inner.find_one(id).await?;
                let intended = self
                    .device
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|x| x.clone().to_generic());
                self.first_device_read_exact
                    .store(actual == intended, Ordering::SeqCst);
                return Ok(actual);
            }
        }
        let log = self
            .log
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|x| x.obj_id() == id);
        if log {
            let count = self.log_reads.fetch_add(1, Ordering::SeqCst) + 1;
            let plan = self.plan.lock().unwrap().clone();
            if self.armed.load(Ordering::SeqCst)
                && plan.is_some_and(|p| p.fault == Fault::LogReadback)
            {
                if count == 1 {
                    let actual = self.inner.find_one(id).await?;
                    ensure!(
                        actual
                            == self
                                .log
                                .lock()
                                .unwrap()
                                .as_ref()
                                .map(|x| x.clone().to_generic()),
                        "first observer Find did not read actual saved intended SsLog"
                    );
                    self.observer_log_forwarded.fetch_add(1, Ordering::SeqCst);
                    return Ok(actual);
                }
                if count == 2 {
                    self.publisher_read_hits.fetch_add(1, Ordering::SeqCst);
                    self.fault_hits.fetch_add(1, Ordering::SeqCst);
                    anyhow::bail!("actual publisher SECOND SsLog readback PRIVATE_SENTINEL");
                }
            }
        }
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        Ok(self.find_one(id).await?.map(|x| x.obj_id()))
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for ClaimRepo<R> {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(false, event.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(true, event.to_generic()).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for ClaimRepo<R> {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for ClaimRepo<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for ClaimRepo<R> {}
