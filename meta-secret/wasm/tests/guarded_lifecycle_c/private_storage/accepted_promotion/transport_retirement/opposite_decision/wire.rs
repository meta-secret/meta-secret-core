use super::*;
#[derive(Clone)]
pub(super) struct Noop {
    pub order: usize,
    pub authorization: DeviceTransportNoop,
    pub durable: bool,
    pub outer_read: bool,
    pub scrubbed: bool,
    pub receipt: Option<DeviceTransportNoopReceipt>,
}
#[derive(Clone)]
pub(super) struct Tail {
    pub order: usize,
    pub after_receipt: bool,
    pub response: ServerTailResponse,
}
pub(super) struct Forward<R: KvLogEventRepo> {
    actual: Arc<ActualWire<R>>,
    repo: Arc<repo::Outer<R>>,
    witness: ArtifactId,
    deleted: MetaPasswordId,
    server_repo: Arc<InMemKvLogEventRepo>,
    pub attempts: Mutex<Vec<SyncRequest>>,
    pub noops: Mutex<Vec<Noop>>,
    pub tails: Mutex<Vec<Tail>>,
}
impl<R: KvLogEventRepo> Forward<R> {
    pub fn new(case: &ActualCase<R>, repo: Arc<repo::Outer<R>>) -> Self {
        Self {
            actual: case.wire.clone(),
            repo,
            witness: case.h.state.witness_id(),
            deleted: case.h.state.passes[0].clone(),
            server_repo: case.h.state.server_repo.clone(),
            attempts: Mutex::new(vec![]),
            noops: Mutex::new(vec![]),
            tails: Mutex::new(vec![]),
        }
    }
}
impl<R: KvLogEventRepo> SyncProtocol for Forward<R> {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let order = tick(&self.repo.clock);
        self.attempts.lock().unwrap().push(request.clone());
        let w: LocalGuardedPreparationObject = self
            .repo
            .physical
            .find_one(self.witness.clone())
            .await?
            .context("physical witness")?
            .try_into()?;
        let technical = w.0.value.transport_noop.as_ref();
        let tail = matches!(&request,SyncRequest::Read(r) if matches!(&**r,ReadSyncRequest::ServerTail(_)));
        let after_receipt = technical.is_some_and(|n| n.receipt.is_some());
        let mut noop = if let SyncRequest::Write(r) = &request
            && let WriteSyncRequest::DeviceTransportNoop(auth) = &**r
        {
            Some(Noop {
                order,
                authorization: auth.clone(),
                durable: technical.is_some_and(|n| n.authorization == *auth),
                outer_read: self.repo.witness_read(auth, order),
                scrubbed: w.0.value.payload.is_none()
                    && checks::ordinary_clean(&self.repo.inner.snapshot().await?, &self.deleted),
                receipt: None,
            })
        } else {
            None
        };
        // Every request, including actual SS data, forwards unchanged. No prefix cut or stub.
        let result = self.actual.send(request).await;
        if let Some(call) = &mut noop
            && let Ok(DataSyncResponse::DeviceTransportNoop(receipt)) = &result
        {
            let actual = self
                .server_repo
                .find_one(call.authorization.event_id.clone())
                .await?
                .context("actual server marker")?;
            let GenericKvLogEvent::DeviceTransportNoop(marker) = actual else {
                anyhow::bail!("real server Noop absent")
            };
            ensure!(
                marker.receipt() == *receipt && marker.0.value.authorization == call.authorization,
                "actual Noop response differs"
            );
            call.receipt = Some(receipt.clone());
        }
        if let Some(call) = noop {
            self.noops.lock().unwrap().push(call);
        }
        if tail && let Ok(DataSyncResponse::ServerTailResponse(response)) = &result {
            self.tails.lock().unwrap().push(Tail {
                order,
                after_receipt,
                response: (**response).clone(),
            });
        }
        result
    }
}
