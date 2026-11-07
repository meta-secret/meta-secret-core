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
pub(super) struct Read {
    pub order: usize,
    pub id: ArtifactId,
    pub event: Option<GenericKvLogEvent>,
}
pub(super) struct Write {
    pub forward: usize,
    pub done: usize,
    pub success: bool,
    pub exact: bool,
    pub event: GenericKvLogEvent,
}
pub(super) struct Clean {
    pub entry: usize,
    pub done: usize,
    pub client: bool,
    pub sync: bool,
    pub same: bool,
    pub declined: bool,
    pub old_member: bool,
    pub emptied: bool,
}
type OwnerGuards = (Arc<async_lock::Mutex<()>>, Arc<async_lock::Mutex<()>>);
pub(super) struct Repo {
    pub inner: Arc<prior_repo::Repo>,
    pub physical: Arc<InMemKvLogEventRepo>,
    pub calls: Mutex<Vec<Call>>,
    pub reads: Mutex<Vec<Read>>,
    pub writes: Mutex<Vec<Write>>,
    pub cleans: Mutex<Vec<Clean>>,
    pub guards: Mutex<Option<OwnerGuards>>,
    pub credential_owner_seen: AtomicBool,
    pub unauthorized: AtomicBool,
    pub fault: AtomicBool,
    pub fault_hit: AtomicBool,
    pub fault_snapshot: Mutex<Option<HashMap<ArtifactId, GenericKvLogEvent>>>,
    pub fault_owner: AtomicBool,
    actor: UserCreds,
    master: TransportSk,
}
impl Repo {
    pub fn new(p: &prior::Peer) -> Self {
        Self {
            inner: p.repo.clone(),
            physical: p.physical.clone(),
            calls: Mutex::new(vec![]),
            reads: Mutex::new(vec![]),
            writes: Mutex::new(vec![]),
            cleans: Mutex::new(vec![]),
            guards: Mutex::new(None),
            credential_owner_seen: false.into(),
            unauthorized: false.into(),
            fault: false.into(),
            fault_hit: false.into(),
            fault_snapshot: Mutex::new(None),
            fault_owner: false.into(),
            actor: p.actor.clone(),
            master: p.master.clone(),
        }
    }
    pub fn tick(&self) -> usize {
        self.inner.tick()
    }
    pub fn owners(&self) -> (bool, bool) {
        self.guards
            .lock()
            .unwrap()
            .as_ref()
            .map(|(c, s)| (c.try_lock().is_none(), s.try_lock().is_none()))
            .unwrap_or((false, false))
    }
    pub fn call(&self, op: Op) -> usize {
        let order = self.tick();
        self.calls.lock().unwrap().push(Call { order, op });
        order
    }
    pub fn reset(&self) {
        self.calls.lock().unwrap().clear();
        self.reads.lock().unwrap().clear();
        self.writes.lock().unwrap().clear();
        self.cleans.lock().unwrap().clear();
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
        self.call(Op::Write(replace, Box::new(event.clone())));
        let forward = self.tick();
        let r = if replace {
            self.inner.replace(event.clone()).await
        } else {
            self.inner.save(event.clone()).await
        };
        let done = self.tick();
        let exact = self.physical.find_one(event.obj_id()).await?.as_ref() == Some(&event);
        self.writes.lock().unwrap().push(Write {
            forward,
            done,
            success: r.is_ok(),
            exact,
            event,
        });
        r
    }
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.call(Op::Find(id.clone()));
        let (client, sync) = self.owners();
        if id.fqdn == ArtifactId::from(UserCredsDescriptor).fqdn && client {
            self.credential_owner_seen.store(true, Ordering::SeqCst);
        }
        let status = ArtifactId::from(VaultStatusDescriptor::from(self.actor.user_id()));
        if id == status
            && self.unauthorized.load(Ordering::SeqCst)
            && client
            && sync
            && self.fault.swap(false, Ordering::SeqCst)
        {
            *self.fault_snapshot.lock().unwrap() = Some(self.physical.get_db().await);
            self.fault_owner.store(true, Ordering::SeqCst);
            self.fault_hit.store(true, Ordering::SeqCst);
            return Err(ProofReadError.into());
        }
        let event = self.inner.find_one(id.clone()).await?;
        self.reads.lock().unwrap().push(Read {
            order: self.tick(),
            id,
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
        let (client, sync) = self.owners();
        let same = self.raw_creds().await.ok().flatten().as_ref() == Some(&self.actor);
        let db = self.physical.get_db().await;
        let declined=db.values().any(|e|matches!(e,GenericKvLogEvent::VaultStatus(s) if matches!(s.clone().status(),VaultStatus::Outsider(o) if o.user_data==self.actor.user() && o.status==UserDataOutsiderStatus::Declined)));
        let old_member=db.values().any(|e|matches!(e,GenericKvLogEvent::Vault(v) if v.0.value.vault_name==self.actor.vault_name && v.0.value.is_member(self.actor.device_id())));
        self.inner.db_clean_up().await;
        let emptied = self.physical.get_db().await.is_empty();
        let done = self.tick();
        self.cleans.lock().unwrap().push(Clean {
            entry,
            done,
            client,
            sync,
            same,
            declined,
            old_member,
            emptied,
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
