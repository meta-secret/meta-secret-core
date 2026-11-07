use super::*;

pub(super) struct Forward<R: KvLogEventRepo> {
    pub remote: Remote,
    local: Arc<R>,
    owner: UserData,
    plan: Mutex<Option<(Identity, DeviceId)>>,
    pub vault_reads: AtomicUsize,
    pub first_real_three: AtomicBool,
    pub hits: AtomicUsize,
    pub imported_three_at_fault: AtomicBool,
    pub uploads: Mutex<Vec<GenericKvLogEvent>>,
    pub actions: Mutex<Vec<SignedAction>>,
    pub noops: AtomicUsize,
}
impl<R: KvLogEventRepo> Forward<R> {
    pub fn new(remote: Remote, local: Arc<R>, owner: UserData) -> Self {
        Self {
            remote,
            local,
            owner,
            plan: Mutex::new(None),
            vault_reads: 0.into(),
            first_real_three: false.into(),
            hits: 0.into(),
            imported_three_at_fault: false.into(),
            uploads: Mutex::new(vec![]),
            actions: Mutex::new(vec![]),
            noops: 0.into(),
        }
    }
    pub fn arm(&self, identity: Identity, joined: DeviceId) {
        self.vault_reads.store(0, Ordering::SeqCst);
        self.first_real_three.store(false, Ordering::SeqCst);
        self.uploads.lock().unwrap().clear();
        self.actions.lock().unwrap().clear();
        if identity.kind == FaultKind::SecondVaultBefore {
            *self.plan.lock().unwrap() = Some((identity, joined));
        }
    }
}
impl<R: KvLogEventRepo> SyncProtocol for Forward<R> {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let is_vault = matches!(&request, SyncRequest::Read(read)
            if matches!(&**read, ReadSyncRequest::Vault(v) if v.sender==self.owner));
        let nth = if is_vault {
            self.vault_reads.fetch_add(1, Ordering::SeqCst) + 1
        } else {
            0
        };
        let plan = self.plan.lock().unwrap().clone();
        if nth == 2
            && let Some((identity, joined)) = plan
        {
            let actual = PersistentVault::from(Arc::new(PersistentObject::new(self.local.clone())))
                .get_vault(self.owner.vault_name())
                .await?;
            ensure!(
                self.first_real_three.load(Ordering::SeqCst)
                    && actual.0.value.members().len() == 3
                    && actual.0.value.is_member(&joined),
                "second Vault error must follow actual first Data and durable import3"
            );
            self.imported_three_at_fault.store(true, Ordering::SeqCst);
            self.hits.fetch_add(1, Ordering::SeqCst);
            self.plan.lock().unwrap().take();
            return Err(identity.error()); // Before actual second server read; no fake Data.
        }
        if let SyncRequest::Write(write) = &request {
            match &**write {
                WriteSyncRequest::Event(action) => {
                    self.actions.lock().unwrap().push(action.clone());
                    match &action.payload {
                        SignedActionPayload::Event(event) => {
                            self.uploads.lock().unwrap().push(event.clone())
                        }
                        SignedActionPayload::GuardedRemovalDecision(g) => self
                            .uploads
                            .lock()
                            .unwrap()
                            .push(g.event.clone().to_generic()),
                        SignedActionPayload::RecoveryCompletion(_) => {}
                    }
                }
                WriteSyncRequest::DeviceTransportNoop(_) => {
                    self.noops.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        let result = Box::pin(self.remote.handle_client_request(request)).await;
        if nth == 1
            && let Ok(DataSyncResponse::Data(data)) = &result
        {
            let has_three = data.0.iter().any(|event| {
                matches!(event,
                GenericKvLogEvent::Vault(v) if v.0.value.members().len()==3)
            });
            self.first_real_three.store(has_three, Ordering::SeqCst);
        }
        result
    }
}
