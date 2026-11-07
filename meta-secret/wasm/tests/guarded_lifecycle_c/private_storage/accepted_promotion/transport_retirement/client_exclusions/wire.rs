use super::*;

#[derive(Debug)]
pub(super) struct PreNoop;
#[derive(Debug)]
pub(super) struct SsBoundary;
macro_rules! stop_error {
    ($t:ty, $s:literal) => {
        impl std::fmt::Display for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str($s)
            }
        }
        impl std::error::Error for $t {}
    };
}
stop_error!(PreNoop, "actual client before first Noop forwarding");
stop_error!(SsBoundary, "actual client FIRST SS component boundary");
pub(super) struct Noop {
    pub order: usize,
    pub authorization: DeviceTransportNoop,
    pub durable: bool,
    pub readback: bool,
    pub private_clean: bool,
    pub ordinary_clean: bool,
    pub forwarded: bool,
    pub conflict: bool,
    pub unauthorized: bool,
    pub receipt: Option<DeviceTransportNoopReceipt>,
}
pub(super) struct Tail {
    pub order: usize,
    pub response: ServerTailResponse,
}
pub(super) struct Forward {
    pub server: Arc<ServerApp<InMemKvLogEventRepo>>,
    pub repo: Arc<observer::Repo>,
    pub owner: UserId,
    pub selected: Mutex<Option<(ArtifactId, Option<MetaPasswordId>)>>,
    pub stop_noop: AtomicBool,
    pub stop_ss: AtomicBool,
    pub noops: Mutex<Vec<Noop>>,
    pub tails: Mutex<Vec<Tail>>,
    pub actions: Mutex<Vec<SignedAction>>,
    pub writes: Mutex<Vec<SyncRequest>>,
    pub cuts: Mutex<Vec<(usize, UserId, ArtifactId)>>,
    pub active_inputs: Mutex<Vec<GenericKvLogEvent>>,
    pub active_at_cut: Mutex<Vec<bool>>,
}
impl Forward {
    pub fn new(
        server: Arc<ServerApp<InMemKvLogEventRepo>>,
        repo: Arc<observer::Repo>,
        owner: UserId,
    ) -> Self {
        Self {
            server,
            repo,
            owner,
            selected: Mutex::new(None),
            stop_noop: false.into(),
            stop_ss: false.into(),
            noops: Mutex::new(vec![]),
            tails: Mutex::new(vec![]),
            actions: Mutex::new(vec![]),
            writes: Mutex::new(vec![]),
            cuts: Mutex::new(vec![]),
            active_inputs: Mutex::new(vec![]),
            active_at_cut: Mutex::new(vec![]),
        }
    }
    pub fn reset(&self) {
        self.noops.lock().unwrap().clear();
        self.tails.lock().unwrap().clear();
        self.actions.lock().unwrap().clear();
        self.writes.lock().unwrap().clear();
        self.cuts.lock().unwrap().clear();
        self.active_at_cut.lock().unwrap().clear();
    }
    pub fn stopped_ss(&self, r: &Result<()>) -> bool {
        r.as_ref().err().is_some_and(|e| e.is::<SsBoundary>())
            && self.cuts.lock().unwrap().len() == 1
    }
}
impl SyncProtocol for Forward {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let order = self.repo.tick();
        if let SyncRequest::Read(r) = &request
            && let ReadSyncRequest::SsRequest(s) = &**r
            && self.stop_ss.load(Ordering::SeqCst)
            && s.sender.user_id() == self.owner
        {
            let active = self.active_inputs.lock().unwrap().clone();
            let mut preserved = true;
            for e in active {
                preserved &= self.repo.physical.find_one(e.obj_id()).await? == Some(e);
            }
            self.active_at_cut.lock().unwrap().push(preserved);
            self.cuts
                .lock()
                .unwrap()
                .push((order, s.sender.user_id(), s.ss_log.clone()));
            return Err(anyhow::Error::new(SsBoundary)); // No response/data import is manufactured.
        }
        let mut noop = None;
        let tail = matches!(&request, SyncRequest::Read(r) if matches!(&**r, ReadSyncRequest::ServerTail(_)));
        if let SyncRequest::Write(w) = &request {
            self.writes.lock().unwrap().push(request.clone()); // ALL writes before any payload filter.
            match &**w {
                WriteSyncRequest::Event(a) => {
                    self.actions.lock().unwrap().push(a.clone());
                }
                WriteSyncRequest::DeviceTransportNoop(auth) => {
                    let selected = self
                        .selected
                        .lock()
                        .unwrap()
                        .clone()
                        .context("Noop selected actual witness")?;
                    let raw = self
                        .repo
                        .physical
                        .find_one(selected.0)
                        .await?
                        .context("actual presend witness")?;
                    let w: LocalGuardedPreparationObject = raw.clone().try_into()?;
                    let readback = self.repo.exact_read_before(&raw, order);
                    let ordinary_clean = if let Some(pass) = &selected.1 {
                        checks::ordinary_clean(&self.repo.physical.get_db().await, pass)
                    } else {
                        true
                    };
                    noop = Some(Noop {
                        order,
                        authorization: auth.clone(),
                        durable: w
                            .0
                            .value
                            .transport_noop
                            .as_ref()
                            .is_some_and(|n| n.authorization == *auth),
                        readback,
                        private_clean: w.0.value.payload.is_none(),
                        ordinary_clean,
                        forwarded: false,
                        conflict: false,
                        unauthorized: false,
                        receipt: None,
                    });
                    if self.stop_noop.load(Ordering::SeqCst) {
                        self.noops.lock().unwrap().push(noop.take().unwrap());
                        return Err(anyhow::Error::new(PreNoop));
                    }
                }
            }
        }
        let result = Box::pin(self.server.handle_client_request(request)).await;
        if let Some(mut n) = noop {
            n.forwarded = true;
            if let Ok(DataSyncResponse::DeviceTransportNoop(r)) = &result {
                n.receipt = Some(r.clone());
            }
            if let Err(e) = &result {
                n.conflict = e
                    .chain()
                    .any(|v| v.to_string() == "DeviceTransportNoopConflict");
                n.unauthorized = e
                    .chain()
                    .any(|v| v.to_string() == "DeviceTransportNoopUnauthorized");
            }
            self.noops.lock().unwrap().push(n);
        }
        if tail && let Ok(DataSyncResponse::ServerTailResponse(r)) = &result {
            self.tails.lock().unwrap().push(Tail {
                order,
                response: (**r).clone(),
            });
        }
        result
    }
}
