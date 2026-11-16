use super::*;

#[derive(Debug)]
pub(super) struct SsBoundary;
impl std::fmt::Display for SsBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("receipt tail SS component boundary")
    }
}
impl std::error::Error for SsBoundary {}
#[derive(Debug)]
pub(super) struct ResponseLost;
impl std::fmt::Display for ResponseLost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("actual receipt tail response lost")
    }
}
impl std::error::Error for ResponseLost {}
#[derive(Clone)]
pub(super) struct Noop {
    pub order: usize,
    pub auth: DeviceTransportNoop,
    pub durable: bool,
    pub outer_read: bool,
    pub private_clean: bool,
    pub ordinary_clean: bool,
    pub receipt: Option<DeviceTransportNoopReceipt>,
    pub returned: bool,
}
#[derive(Clone)]
pub(super) struct Tail {
    pub order: usize,
    pub receipt_durable_before: bool,
    pub value: ServerTailResponse,
    pub returned: bool,
}
pub(super) struct Cut {
    pub order: usize,
    pub sender: UserData,
    pub stream: ArtifactId,
}
pub(super) struct Forward<R: KvLogEventRepo> {
    actual: Arc<ActualWire<R>>,
    repo: Arc<repo::Outer<R>>,
    server_repo: Arc<InMemKvLogEventRepo>,
    owner: UserData,
    deleted: MetaPasswordId,
    mode: Mode,
    enabled: AtomicBool,
    targeted: AtomicBool,
    pub hits: AtomicUsize,
    pub attempts: Mutex<Vec<SyncRequest>>,
    pub actions: Mutex<Vec<SignedAction>>,
    pub noops: Mutex<Vec<Noop>>,
    pub tails: Mutex<Vec<Tail>>,
    pub cuts: Mutex<Vec<Cut>>,
}
impl<R: KvLogEventRepo> Forward<R> {
    pub fn new(case: &GenericCase<R>, repo: Arc<repo::Outer<R>>, mode: Mode) -> Self {
        Self {
            actual: case.wire.clone(),
            repo,
            server_repo: case.h.state.server_repo.clone(),
            owner: case.h.state.b.user(),
            deleted: case.h.state.passes[0].clone(),
            mode,
            enabled: true.into(),
            targeted: false.into(),
            hits: 0.into(),
            attempts: Mutex::new(vec![]),
            actions: Mutex::new(vec![]),
            noops: Mutex::new(vec![]),
            tails: Mutex::new(vec![]),
            cuts: Mutex::new(vec![]),
        }
    }
    pub fn clear(&self) {
        self.enabled.store(false, Ordering::SeqCst);
    }
    pub fn stopped(&self, result: &Result<()>, previous: usize) -> bool {
        let cuts = self.cuts.lock().unwrap();
        result.as_ref().err().is_some_and(|e| e.is::<SsBoundary>())
            && cuts.len() == previous + 1
            && cuts.get(previous).is_some_and(|c| {
                c.sender == self.owner
                    && c.stream.fqdn
                        == ArtifactId::from(SsLogDescriptor::from(self.owner.vault_name.clone()))
                            .fqdn
            })
    }
}
impl<R: KvLogEventRepo> SyncProtocol for Forward<R> {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let order = tick(&self.repo.clock);
        self.attempts.lock().unwrap().push(request.clone()); // BEFORE transport scope filters.
        if let SyncRequest::Read(read) = &request
            && let ReadSyncRequest::SsRequest(ss) = &**read
            && ss.sender == self.owner
        {
            self.cuts.lock().unwrap().push(Cut {
                order,
                sender: ss.sender.clone(),
                stream: ss.ss_log.clone(),
            });
            return Err(anyhow::Error::new(SsBoundary));
        }
        let is_tail = matches!(&request,SyncRequest::Read(r) if matches!(&**r,ReadSyncRequest::ServerTail(_)));
        let witness = self.repo.witness().await?;
        let technical = witness.0.value.transport_noop.as_ref();
        let receipt_before = technical.and_then(|n| n.receipt.as_ref());
        let receipt_read_before = receipt_before.is_some_and(|r| self.repo.receipt_read(r, order));
        let mut noop = None;
        if let SyncRequest::Write(write) = &request {
            match &**write {
                WriteSyncRequest::Event(action) => {
                    self.actions.lock().unwrap().push(action.clone())
                }
                WriteSyncRequest::DeviceTransportNoop(auth) => {
                    noop = Some(Noop {
                        order,
                        auth: auth.clone(),
                        durable: technical.is_some_and(|n| n.authorization == *auth),
                        outer_read: self.repo.auth_read(auth, order),
                        private_clean: witness.0.value.payload.is_none(),
                        ordinary_clean: checks::ordinary_clean(
                            &self.repo.inner.snapshot().await?,
                            &self.deleted,
                        ),
                        receipt: None,
                        returned: false,
                    });
                }
            }
        }
        let result = self.actual.send(request).await;
        let mut lose = false;
        if let Some(mut call) = noop {
            if let Ok(DataSyncResponse::DeviceTransportNoop(receipt)) = &result {
                let raw = self
                    .server_repo
                    .find_one(call.auth.event_id.clone())
                    .await?
                    .context("actual Noop server commit")?;
                let GenericKvLogEvent::DeviceTransportNoop(marker) = raw else {
                    anyhow::bail!("server did not commit actual Noop marker")
                };
                ensure!(
                    marker.0.value.authorization == call.auth && marker.receipt() == *receipt,
                    "real returned receipt differs from committed marker"
                );
                call.receipt = Some(receipt.clone());
                if self.mode == Mode::ServerReplyLoss
                    && self.enabled.load(Ordering::SeqCst)
                    && !self.targeted.swap(true, Ordering::SeqCst)
                {
                    self.hits.fetch_add(1, Ordering::SeqCst);
                    lose = true;
                }
                call.returned = !lose;
            }
            self.noops.lock().unwrap().push(call);
        }
        if is_tail && let Ok(DataSyncResponse::ServerTailResponse(value)) = &result {
            if self.mode == Mode::TailLoss
                && self.enabled.load(Ordering::SeqCst)
                && receipt_before.is_some()
                && receipt_read_before
                && !self.targeted.swap(true, Ordering::SeqCst)
            {
                ensure!(
                    value
                        .device_log_tail
                        .as_ref()
                        .is_some_and(|tail| tail.fqdn == self.repo.origin.event_id.fqdn
                            && tail.id.curr > self.repo.origin.event_id.id.curr),
                    "actual postreceipt Tail did not advance"
                );
                self.hits.fetch_add(1, Ordering::SeqCst);
                lose = true;
            }
            self.tails.lock().unwrap().push(Tail {
                order,
                receipt_durable_before: receipt_before.is_some(),
                value: (**value).clone(),
                returned: !lose,
            });
        }
        if lose {
            return Err(anyhow::Error::new(ResponseLost));
        }
        result
    }
}
