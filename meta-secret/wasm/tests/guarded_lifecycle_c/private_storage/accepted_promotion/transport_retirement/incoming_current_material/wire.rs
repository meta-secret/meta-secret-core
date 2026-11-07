use super::*;
pub(super) struct Call {
    pub order: usize,
    pub request: SyncRequest,
}
pub(super) struct Reply {
    pub order: usize,
    pub request: SyncRequest,
    pub response: DataSyncResponse,
}
pub(super) struct Noop {
    pub entry: usize,
    pub done: usize,
    pub auth: DeviceTransportNoop,
    pub durable: bool,
    pub readback: bool,
    pub private_clean: bool,
    pub ordinary_clean: bool,
    pub receipt: Option<DeviceTransportNoopReceipt>,
}
pub(super) struct Boundary {
    pub order: usize,
    pub snapshot: LocalGuardedPreparationObject,
    pub body: GenericKvLogEvent,
    pub index: GenericKvLogEvent,
    pub registry: GenericKvLogEvent,
}
pub(super) struct Forward {
    inner: Arc<super::super::super::wire::Forward<local_repo::Repo>>,
    pub repo: Arc<repo::Repo>,
    witness_id: ArtifactId,
    index_id: ArtifactId,
    owner: UserId,
    deleted: MetaPasswordId,
    body_id: ArtifactId,
    pub calls: Mutex<Vec<Call>>,
    pub replies: Mutex<Vec<Reply>>,
    pub noops: Mutex<Vec<Noop>>,
    pub boundaries: Mutex<Vec<Boundary>>,
}
impl Forward {
    pub fn new(case: &Case, repo: Arc<repo::Repo>, origin: &GuardedCommandOrigin) -> Self {
        Self {
            inner: case.wire.clone(),
            repo,
            witness_id: case.h.state.witness_id(),
            index_id: case.h.state.index_id(),
            owner: case.h.state.b.user_id(),
            deleted: case.h.state.passes[0].clone(),
            body_id: origin.event_id.clone(),
            calls: Mutex::new(vec![]),
            replies: Mutex::new(vec![]),
            noops: Mutex::new(vec![]),
            boundaries: Mutex::new(vec![]),
        }
    }
}
impl SyncProtocol for Forward {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let entry = self.repo.tick();
        self.calls.lock().unwrap().push(Call {
            order: entry,
            request: request.clone(),
        });
        let mut noop = None;
        if let SyncRequest::Write(w) = &request
            && let WriteSyncRequest::DeviceTransportNoop(auth) = &**w
        {
            let raw = self
                .repo
                .physical
                .find_one(self.witness_id.clone())
                .await?
                .context("actual Noop witness")?;
            let w = witness(raw.clone())?;
            let readback = self
                .repo
                .reads
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.order < entry && r.event.as_ref() == Some(&raw));
            noop = Some(Noop {
                entry,
                done: 0,
                auth: auth.clone(),
                durable: w
                    .0
                    .value
                    .transport_noop
                    .as_ref()
                    .is_some_and(|n| n.authorization == *auth),
                readback,
                private_clean: w.0.value.payload.is_none(),
                ordinary_clean: checks::ordinary_clean(
                    &self.repo.physical.get_db().await,
                    &self.deleted,
                ),
                receipt: None,
            });
        }
        if matches!(&request,SyncRequest::Read(r)if matches!(&**r,ReadSyncRequest::SsRequest(s)if s.sender.user_id()==self.owner))
        {
            let snapshot = witness(
                self.repo
                    .physical
                    .find_one(self.witness_id.clone())
                    .await?
                    .context("actual pre-SS compact")?,
            )?;
            let body = self
                .repo
                .physical
                .find_one(self.body_id.clone())
                .await?
                .context("actual retained redacted N")?;
            let index = self
                .repo
                .physical
                .find_one(self.index_id.clone())
                .await?
                .context("actual index before SS")?;
            let registry = PersistentRemovalEnvelope {
                p_obj: Arc::new(PersistentObject::new(self.repo.physical.clone())),
            }
            .load(&self.owner)
            .await?
            .context("actual registry before SS")?
            .to_generic();
            self.boundaries.lock().unwrap().push(Boundary {
                order: entry,
                snapshot,
                body,
                index,
                registry,
            });
        }
        // Passive forwarding ONLY. No stop, synthetic Data, reply conversion or error normalization.
        let result = self.inner.send(request.clone()).await;
        let done = self.repo.tick();
        if let Some(mut n) = noop {
            n.done = done;
            if let Ok(DataSyncResponse::DeviceTransportNoop(receipt)) = &result {
                n.receipt = Some(receipt.clone());
            }
            self.noops.lock().unwrap().push(n);
        }
        if let Ok(response) = &result {
            self.replies.lock().unwrap().push(Reply {
                order: done,
                request,
                response: response.clone(),
            });
        }
        result
    }
}
