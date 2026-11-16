use super::*;
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsLogDescriptor;

#[derive(Debug)]
struct SharedSecretCutpoint;
impl std::fmt::Display for SharedSecretCutpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("C3 shared-secret component boundary")
    }
}
impl std::error::Error for SharedSecretCutpoint {}
pub(super) struct Cutpoint {
    pub order: usize,
    pub sender: UserId,
    pub ss_log: ArtifactId,
}

pub(super) struct NoopCall {
    pub order: usize,
    pub authorization: DeviceTransportNoop,
    pub actual_auth_durable_before: bool,
    pub production_auth_readback_before: bool,
    pub private_scrub_before: bool,
    pub ordinary_scrub_before: bool,
    pub receipt: Option<DeviceTransportNoopReceipt>,
}
pub(super) struct Tail {
    pub order: usize,
    pub after_actual_receipt: bool,
    pub actual: ServerTailResponse,
}
pub(super) struct Forward {
    inner: Arc<super::super::wire::Forward<local_repo::Repo>>,
    local: Arc<local_repo::Repo>,
    witness_id: ArtifactId,
    deleted: MetaPasswordId,
    owner: UserId,
    order: AtomicUsize,
    pub noops: Mutex<Vec<NoopCall>>,
    pub tails: Mutex<Vec<Tail>>,
    pub device_actions: Mutex<Vec<SignedAction>>,
    pub vault_reads: AtomicUsize,
    pub vault_read_orders: Mutex<Vec<usize>>,
    pub device_action_orders: Mutex<Vec<usize>>,
    pub cuts: Mutex<Vec<Cutpoint>>,
}
impl Forward {
    pub fn new(case: &Case, repo: Arc<local_repo::Repo>) -> Self {
        Self {
            inner: case.wire.clone(),
            local: repo,
            witness_id: case.h.state.witness_id(),
            deleted: case.h.state.passes[0].clone(),
            owner: case.h.state.b.user_id(),
            order: 0.into(),
            noops: Mutex::new(vec![]),
            tails: Mutex::new(vec![]),
            device_actions: Mutex::new(vec![]),
            vault_reads: 0.into(),
            vault_read_orders: Mutex::new(vec![]),
            device_action_orders: Mutex::new(vec![]),
            cuts: Mutex::new(vec![]),
        }
    }
    pub fn stopped(&self, result: &Result<()>, previous: usize) -> bool {
        let cuts = self.cuts.lock().unwrap();
        let stream = ArtifactId::from(SsLogDescriptor::from(self.owner.vault_name.clone())).fqdn;
        result
            .as_ref()
            .err()
            .is_some_and(|error| error.is::<SharedSecretCutpoint>())
            && cuts.len() == previous + 1
            && cuts.get(previous).is_some_and(|cut| {
                cut.sender == self.owner
                    && cut.ss_log.fqdn == stream
                    && self
                        .vault_read_orders
                        .lock()
                        .unwrap()
                        .last()
                        .is_some_and(|order| *order < cut.order)
                    && self
                        .noops
                        .lock()
                        .unwrap()
                        .iter()
                        .all(|noop| noop.order < cut.order)
                    && self
                        .device_action_orders
                        .lock()
                        .unwrap()
                        .iter()
                        .all(|order| *order < cut.order)
            })
    }
}
impl SyncProtocol for Forward {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let order = self.order.fetch_add(1, Ordering::SeqCst) + 1;
        if let SyncRequest::Read(read) = &request
            && let ReadSyncRequest::SsRequest(ss) = &**read
            && ss.sender.user_id() == self.owner
        {
            self.cuts.lock().unwrap().push(Cutpoint {
                order,
                sender: ss.sender.user_id(),
                ss_log: ss.ss_log.clone(),
            });
            // Typed component stop BEFORE delivery/import, never successful SS Data.
            return Err(anyhow::Error::new(SharedSecretCutpoint));
        }
        let mut noop = None;
        let is_tail = matches!(&request, SyncRequest::Read(read) if matches!(&**read, ReadSyncRequest::ServerTail(_)));
        if matches!(&request, SyncRequest::Read(read) if matches!(&**read, ReadSyncRequest::Vault(_)))
        {
            self.vault_reads.fetch_add(1, Ordering::SeqCst);
            self.vault_read_orders.lock().unwrap().push(order);
        }
        if let SyncRequest::Write(write) = &request {
            match &**write {
                WriteSyncRequest::DeviceTransportNoop(auth) => {
                    // Capture BEFORE test-side reads, which bypass the observed layer.
                    let production_auth_readback_before =
                        self.local.auth_reads.lock().unwrap().contains(auth);
                    let actual = self.local.inner.find_one(self.witness_id.clone()).await?;
                    let (actual_auth_durable_before, private_scrub_before) = match actual {
                        Some(GenericKvLogEvent::LocalGuardedPreparation(w)) => (
                            w.0.value
                                .transport_noop
                                .as_ref()
                                .is_some_and(|n| n.authorization == *auth),
                            w.0.value.payload.is_none(),
                        ),
                        _ => (false, false),
                    };
                    noop = Some(NoopCall {
                        order,
                        authorization: auth.clone(),
                        actual_auth_durable_before,
                        production_auth_readback_before,
                        private_scrub_before,
                        ordinary_scrub_before: checks::ordinary_clean(
                            &self.local.inner.get_db().await,
                            &self.deleted,
                        ),
                        receipt: None,
                    });
                }
                WriteSyncRequest::Event(action)
                    if matches!(
                        &action.payload,
                        SignedActionPayload::GuardedRemovalDecision(_)
                            | SignedActionPayload::Event(GenericKvLogEvent::DeviceLog(_))
                    ) =>
                {
                    self.device_actions.lock().unwrap().push(action.clone());
                    self.device_action_orders.lock().unwrap().push(order);
                }
                _ => {}
            }
        }
        let after_actual_receipt = self
            .noops
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.receipt.is_some());
        let result = self.inner.send(request).await;
        if let Some(mut call) = noop {
            if let Ok(DataSyncResponse::DeviceTransportNoop(receipt)) = &result {
                call.receipt = Some(receipt.clone());
            }
            self.noops.lock().unwrap().push(call);
        }
        if is_tail && let Ok(DataSyncResponse::ServerTailResponse(actual)) = &result {
            self.tails.lock().unwrap().push(Tail {
                order,
                after_actual_receipt,
                actual: (**actual).clone(),
            });
        }
        result // Original real response/error, never a synthetic successful Noop/tail.
    }
}
