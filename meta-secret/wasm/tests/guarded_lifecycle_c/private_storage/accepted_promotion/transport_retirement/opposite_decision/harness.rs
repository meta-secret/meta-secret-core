use super::*;
pub(super) struct Harness<R: KvLogEventRepo> {
    pub case: ActualCase<R>,
    pub physical: Arc<R>,
    pub repo: Arc<repo::Outer<R>>,
    pub wire: Arc<wire::Forward<R>>,
    pub p: Arc<PersistentObject<repo::Outer<R>>>,
    pub gateway: Arc<SyncGateway<repo::Outer<R>, wire::Forward<R>>>,
}
impl<R: KvLogEventRepo> Harness<R> {
    pub async fn fresh(physical: Arc<R>) -> Result<Self> {
        let case = ActualCase::new(physical.clone()).await?;
        let (receipt, decline) = case.prepare(false).await?;
        let w = case.h.witness().await?;
        crypto::decline(&case, &w, &decline).await?;
        ensure!(
            w.0.value
                .origin
                .as_ref()
                .is_some_and(|o| o.event_id == receipt.event_id && !o.accepted)
                && case
                    .h
                    .state
                    .server_repo
                    .find_one(receipt.event_id)
                    .await?
                    .is_none(),
            "real Decline source was sent before settlement"
        );
        tombstone::delete_first(&case).await?;
        Self::from_case(case, physical)
    }
    pub fn from_case(case: ActualCase<R>, physical: Arc<R>) -> Result<Self> {
        let repo = Arc::new(repo::Outer::new(&case, physical.clone()));
        let p = Arc::new(PersistentObject {
            repo: repo.clone(),
            device_log_append_guard: case.h.p.device_log_append_guard.clone(),
            client_operation_guard: case.h.p.client_operation_guard.clone(),
            device_sync_guard: case.h.p.device_sync_guard.clone(),
        });
        ensure!(
            Arc::ptr_eq(
                &p.device_log_append_guard,
                &case.h.p.device_log_append_guard
            ) && Arc::ptr_eq(&p.client_operation_guard, &case.h.p.client_operation_guard)
                && Arc::ptr_eq(&p.device_sync_guard, &case.h.p.device_sync_guard),
            "SAME three public guards differ"
        );
        let wire = Arc::new(wire::Forward::new(&case, repo.clone()));
        let gateway = Arc::new(SyncGateway::new(
            "actual-explicit-opposite".into(),
            p.clone(),
            wire.clone(),
            case.h.state.master.clone(),
        ));
        Ok(Self {
            case,
            physical,
            repo,
            wire,
            p,
            gateway,
        })
    }
    pub async fn sync(&self) -> Result<()> {
        self.gateway.sync(self.case.h.state.b.user()).await
    }
    pub fn service(&self) -> MetaClientService<repo::Outer<R>, wire::Forward<R>> {
        MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer {
                dt: MetaClientRequestTransfer::new(),
            }),
            sync_gateway: self.gateway.clone(),
            state_provider: Arc::new(MetaClientStateProvider::new()),
            p_obj: self.p.clone(),
            device_data: self.case.h.state.b.device(),
            master_key: self.case.h.state.master.clone(),
        }
    }
    pub async fn expect_current(&self) -> Result<GuardedPreparationPayload> {
        let witness = self.case.h.witness().await?;
        let frozen = witness.0.value.payload.context("current FULL")?;
        let original_packets = self
            .case
            .h
            .p
            .repo
            .snapshot()
            .await?
            .into_values()
            .filter_map(|e| match e {
                GenericKvLogEvent::SsWorkflow(p @ SsWorkflowObject::Distribution(_)) => Some(p),
                _ => None,
            })
            .collect();
        *self.case.observations.expected.lock().unwrap() = Some(Expected {
            owner: self.case.h.state.b.user_id(),
            requester: self.case.h.state.a.device_id().clone(),
            target: self.case.h.state.c.device_id().clone(),
            uuid: self.case.h.state.uuid.clone(),
            event_id: witness.0.value.origin.context("current source")?.event_id,
            witness_id: self.case.h.state.witness_id(),
            accepted: true,
            passes: vec![self.case.h.state.passes[1].clone()],
            original_packets,
            frozen: frozen.clone(),
        });
        self.case.observations.live.lock().unwrap().clear();
        self.case.observations.sent.lock().unwrap().clear();
        self.case.observations.uploads.lock().unwrap().clear();
        Ok(frozen)
    }
}
