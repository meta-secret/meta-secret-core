use super::*;

#[derive(Clone)]
pub(super) enum Op {
    Find(ArtifactId),
    Key(ArtifactId),
    Write(bool, Box<GenericKvLogEvent>),
    Delete(ArtifactId),
    TryDelete(ArtifactId),
    Clean,
    Recovery,
}
#[derive(Clone)]
pub(super) struct Call {
    pub order: usize,
    pub op: Op,
}
#[derive(Clone)]
pub(super) struct Read {
    pub order: usize,
    pub event: Option<GenericKvLogEvent>,
}
pub(super) struct Write {
    pub forward: usize,
    pub completed: usize,
    pub event: GenericKvLogEvent,
    pub success: bool,
    pub exact: bool,
}
pub(super) struct Clean {
    pub entry: usize,
    pub done: usize,
    pub client_held: bool,
    pub sync_held: bool,
    pub same_creds: bool,
    pub exact_declined: bool,
    pub old_member: bool,
    pub before_sensitive: bool,
    pub after_empty: bool,
}
#[derive(Clone)]
pub(super) enum SavePoint {
    Recovery(ArtifactId),
    Completed(ClaimId, DeviceId),
}
type OwnerGuards = (Arc<async_lock::Mutex<()>>, Arc<async_lock::Mutex<()>>);
pub(super) struct Repo {
    pub inner: Arc<prior_repo::Repo>,
    pub physical: Arc<InMemKvLogEventRepo>,
    pub calls: Mutex<Vec<Call>>,
    pub reads: Mutex<Vec<Read>>,
    pub cleans: Mutex<Vec<Clean>>,
    pub writes: Mutex<Vec<Write>>,
    pub clean_latch: Latch,
    pub save_latch: Latch,
    pub save_point: Mutex<Option<SavePoint>>,
    pub saved_intent: Mutex<Option<GenericKvLogEvent>>,
    pub guards: Mutex<Option<OwnerGuards>>,
    actor: UserCreds,
    master: TransportSk,
}
impl Repo {
    pub fn new(peer: &prior::Peer) -> Self {
        Self {
            inner: peer.repo.clone(),
            physical: peer.physical.clone(),
            calls: Mutex::new(vec![]),
            reads: Mutex::new(vec![]),
            cleans: Mutex::new(vec![]),
            writes: Mutex::new(vec![]),
            clean_latch: Latch::new(),
            save_latch: Latch::new(),
            save_point: Mutex::new(None),
            saved_intent: Mutex::new(None),
            guards: Mutex::new(None),
            actor: peer.actor.clone(),
            master: peer.master.clone(),
        }
    }
    pub fn tick(&self) -> usize {
        self.inner.tick()
    }
    pub fn call(&self, op: Op) -> usize {
        let order = self.tick();
        self.calls.lock().unwrap().push(Call { order, op });
        order
    }
    pub fn reset(&self) {
        self.calls.lock().unwrap().clear();
        self.reads.lock().unwrap().clear();
        self.cleans.lock().unwrap().clear();
        self.writes.lock().unwrap().clear();
    }
    pub async fn raw_creds(&self) -> Result<Option<UserCreds>> {
        PersistentCredentials {
            p_obj: Arc::new(PersistentObject::new(self.physical.clone())),
            master_key: self.master.clone(),
        }
        .get_user_creds()
        .await
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        self.call(Op::Write(replace, Box::new(event.clone()))); // ALL before any matching predicate.
        let point = self.save_point.lock().unwrap().clone();
        let selected = match (&point, &event) {
            (
                Some(SavePoint::Recovery(id)),
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_)),
            ) => event.obj_id() == *id,
            (Some(SavePoint::Completed(id, actor)), GenericKvLogEvent::SsLog(l)) => {
                l.0.value
                    .claims
                    .get(id)
                    .is_some_and(|c| c.status.get(actor) == Some(&SsDistributionStatus::Delivered))
            }
            _ => false,
        };
        if selected && self.save_latch.armed.load(Ordering::SeqCst) {
            ensure!(
                self.physical.find_one(event.obj_id()).await?.as_ref() != Some(&event),
                "save barrier not a real changed write"
            );
            *self.saved_intent.lock().unwrap() = Some(event.clone());
            self.save_latch.wait().await; // Holds SAME actual production future BEFORE physical forwarding.
        }
        let forward = self.tick(); // AFTER actual barrier release, immediately before forwarding.
        let result = if replace {
            self.inner.replace(event.clone()).await
        } else {
            self.inner.save(event.clone()).await
        };
        let completed = self.tick(); // Actual returned outcome, not the pre-pause attempt timestamp.
        let exact = self.physical.find_one(event.obj_id()).await?.as_ref() == Some(&event);
        self.writes.lock().unwrap().push(Write {
            forward,
            completed,
            event,
            success: result.is_ok(),
            exact,
        });
        result
    }
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.call(Op::Find(id.clone()));
        let event = self.inner.find_one(id).await?;
        self.reads.lock().unwrap().push(Read {
            order: self.tick(),
            event: event.clone(),
        });
        Ok(event)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.call(Op::Key(id.clone()));
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, e: T) -> Result<ArtifactId> {
        self.write(false, e.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, e: T) -> Result<ArtifactId> {
        self.write(true, e.to_generic()).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Repo {
    async fn delete(&self, id: ArtifactId) {
        self.call(Op::Delete(id.clone()));
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.call(Op::TryDelete(id.clone()));
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Repo {
    async fn db_clean_up(&self) {
        let entry = self.call(Op::Clean);
        let (client_held, sync_held) = self
            .guards
            .lock()
            .unwrap()
            .as_ref()
            .map(|(c, s)| (c.try_lock().is_none(), s.try_lock().is_none()))
            .unwrap_or((false, false));
        let before = self.physical.get_db().await;
        let same_creds = self.raw_creds().await.ok().flatten().as_ref() == Some(&self.actor);
        let exact_declined = before.values().any(|e| matches!(e, GenericKvLogEvent::VaultStatus(s)
            if matches!(s.clone().status(), VaultStatus::Outsider(o) if o.user_data == self.actor.user() && o.status == UserDataOutsiderStatus::Declined)));
        let old_member = before.values().any(|e| matches!(e, GenericKvLogEvent::Vault(v)
            if v.0.value.vault_name == self.actor.vault_name && v.0.value.is_member(self.actor.device_id())));
        let before_sensitive = same_creds
            && before.values().any(|e| {
                matches!(
                    e,
                    GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_))
                )
            });
        self.clean_latch.wait().await;
        self.inner.db_clean_up().await;
        let after_empty = self.physical.get_db().await.is_empty();
        let done = self.tick();
        self.cleans.lock().unwrap().push(Clean {
            entry,
            done,
            client_held,
            sync_held,
            same_creds,
            exact_declined,
            old_member,
            before_sensitive,
            after_empty,
        });
    }
}
impl KvLogEventRepo for Repo {
    fn server_recovery_response_store(
        &self,
    ) -> Result<
        &dyn meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore,
    > {
        self.call(Op::Recovery);
        self.inner.server_recovery_response_store()
    }
}
