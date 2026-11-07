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
pub(super) struct NoopEntry {
    pub order: usize,
    pub authorization: DeviceTransportNoop,
    pub private_clean: bool,
    pub ordinary_clean: bool,
    pub auth_readback: bool,
}
pub(super) struct Forward {
    inner: Arc<super::super::super::wire::Forward<local_repo::Repo>>,
    pub repo: Arc<repo::Repo>,
    pub calls: Mutex<Vec<Call>>,
    pub replies: Mutex<Vec<Reply>>,
    pub noops: Mutex<Vec<NoopEntry>>,
    witness_id: ArtifactId,
    deleted: MetaPasswordId,
}
impl Forward {
    pub fn new(case: &Case, repo: Arc<repo::Repo>) -> Self {
        Self {
            inner: case.wire.clone(),
            repo,
            calls: Mutex::new(vec![]),
            replies: Mutex::new(vec![]),
            noops: Mutex::new(vec![]),
            witness_id: case.h.state.witness_id(),
            deleted: case.h.state.passes[0].clone(),
        }
    }
}
impl SyncProtocol for Forward {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let order = self.repo.tick();
        self.calls.lock().unwrap().push(Call {
            order,
            request: request.clone(),
        });
        if let SyncRequest::Write(w) = &request
            && let WriteSyncRequest::DeviceTransportNoop(authorization) = &**w
        {
            let actual = self
                .repo
                .physical
                .find_one(self.witness_id.clone())
                .await?
                .context("actual witness at Noop send")?;
            let record: LocalGuardedPreparationObject = actual.clone().try_into()?;
            let readback = self
                .repo
                .reads
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.order < order && r.event.as_ref() == Some(&actual));
            let ordinary_clean =
                checks::ordinary_clean(&self.repo.physical.get_db().await, &self.deleted);
            self.noops.lock().unwrap().push(NoopEntry {
                order,
                authorization: authorization.clone(),
                private_clean: record.0.value.payload.is_none(),
                ordinary_clean,
                auth_readback: readback
                    && record
                        .0
                        .value
                        .transport_noop
                        .as_ref()
                        .is_some_and(|t| t.authorization == *authorization),
            });
        }
        // No cutpoint, response substitution, success stub, or altered error.
        let result = self.inner.send(request.clone()).await;
        if let Ok(response) = &result {
            self.replies.lock().unwrap().push(Reply {
                order: self.repo.tick(),
                request,
                response: response.clone(),
            });
        }
        result
    }
}
