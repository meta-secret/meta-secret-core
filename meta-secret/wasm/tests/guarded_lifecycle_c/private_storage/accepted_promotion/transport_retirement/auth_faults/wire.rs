use super::*;

#[derive(Debug)]
struct SsBoundary;
impl std::fmt::Display for SsBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("auth4 SS component boundary")
    }
}
impl std::error::Error for SsBoundary {}
pub(super) struct Noop {
    pub order: usize,
    pub authorization: DeviceTransportNoop,
    pub actual_durable: bool,
    pub outer_readback: bool,
    pub private_clean: bool,
    pub ordinary_clean: bool,
    pub receipt: Option<DeviceTransportNoopReceipt>,
}
pub(super) struct Tail {
    pub order: usize,
    pub after_receipt: bool,
    pub value: ServerTailResponse,
}
pub(super) struct Cut {
    pub order: usize,
    pub sender: UserId,
    pub stream: ArtifactId,
}
pub(super) struct Forward {
    actual: Arc<ActualWire>,
    repo: Arc<repo::Repo>,
    owner: UserId,
    deleted: MetaPasswordId,
    pub noops: Mutex<Vec<Noop>>,
    pub tails: Mutex<Vec<Tail>>,
    pub cuts: Mutex<Vec<Cut>>,
    pub actions: Mutex<Vec<SignedAction>>,
}
impl Forward {
    pub fn new(case: &Case, repo: Arc<repo::Repo>) -> Self {
        Self {
            actual: case.wire.clone(),
            repo,
            owner: case.h.state.b.user_id(),
            deleted: case.h.state.passes[0].clone(),
            noops: Mutex::new(vec![]),
            tails: Mutex::new(vec![]),
            cuts: Mutex::new(vec![]),
            actions: Mutex::new(vec![]),
        }
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
impl SyncProtocol for Forward {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let order = tick(&self.repo.clock);
        if let SyncRequest::Read(read) = &request
            && let ReadSyncRequest::SsRequest(ss) = &**read
            && ss.sender.user_id() == self.owner
        {
            self.cuts.lock().unwrap().push(Cut {
                order,
                sender: ss.sender.user_id(),
                stream: ss.ss_log.clone(),
            });
            return Err(anyhow::Error::new(SsBoundary)); // No SS Data or server import is fabricated.
        }
        let mut noop = None;
        let is_tail = matches!(&request,SyncRequest::Read(r) if matches!(&**r,ReadSyncRequest::ServerTail(_)));
        if let SyncRequest::Write(write) = &request {
            match &**write {
                WriteSyncRequest::Event(action) => {
                    self.actions.lock().unwrap().push(action.clone())
                }
                WriteSyncRequest::DeviceTransportNoop(auth) => {
                    let outer_readback = self
                        .repo
                        .reads
                        .lock()
                        .unwrap()
                        .iter()
                        .any(|r| r.order < order && r.authorization == *auth);
                    let actual = self.repo.witness().await?;
                    noop = Some(Noop {
                        order,
                        authorization: auth.clone(),
                        outer_readback,
                        actual_durable: actual
                            .0
                            .value
                            .transport_noop
                            .as_ref()
                            .is_some_and(|n| n.authorization == *auth),
                        private_clean: actual.0.value.payload.is_none(),
                        ordinary_clean: checks::ordinary_clean(
                            &self.repo.physical.get_db().await,
                            &self.deleted,
                        ),
                        receipt: None,
                    });
                }
            }
        }
        let after_receipt = self
            .noops
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.receipt.is_some());
        let result = self.actual.send(request).await;
        if let Some(mut call) = noop {
            if let Ok(DataSyncResponse::DeviceTransportNoop(receipt)) = &result {
                call.receipt = Some(receipt.clone());
            }
            self.noops.lock().unwrap().push(call);
        }
        if is_tail && let Ok(DataSyncResponse::ServerTailResponse(value)) = &result {
            self.tails.lock().unwrap().push(Tail {
                order,
                after_receipt,
                value: (**value).clone(),
            });
        }
        result
    }
}
