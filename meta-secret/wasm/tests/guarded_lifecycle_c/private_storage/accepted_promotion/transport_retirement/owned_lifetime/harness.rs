use super::*;

pub(super) struct Harness {
    pub case: Case,
    pub receipt: RemovalDecisionReceipt,
    pub local: Arc<local_repo::Repo>,
    pub repo: Arc<repo::Repo>,
    pub wire: Arc<wire::Forward>,
    pub p: Arc<PersistentObject<repo::Repo>>,
    pub gateway: Arc<SyncGateway<repo::Repo, wire::Forward>>,
}
impl Harness {
    pub async fn fresh(retire: bool) -> Result<Self> {
        let (case, local) = if retire {
            actor::case().await?
        } else {
            super::super::case().await?
        };
        let (receipt, frozen) = case.prepare(true).await?;
        ensure!(
            frozen.passes.len() == 2,
            "actual packet-bearing Accepted N required"
        );
        let original = case.h.witness().await?;
        private_crypto(&case.h.state, &original)?;
        if retire {
            let origin = original.0.value.origin.as_ref().context("actual N")?;
            let body = local
                .inner
                .find_one(origin.event_id.clone())
                .await?
                .context("actual queued N")?
                .device_log()?;
            local
                .capture_own_body(body, case.h.state.witness_id())
                .await?;
            tombstone::delete_first(&case).await?;
        }
        let repo = Arc::new(repo::Repo::new(&case, &local));
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
            "SAME three owners changed"
        );
        let wire = Arc::new(wire::Forward::new(&case, repo.clone()));
        let gateway = Arc::new(SyncGateway::new(
            "actual-owned-lifetime".into(),
            p.clone(),
            wire.clone(),
            case.h.state.master.clone(),
        ));
        let mut h = Self {
            case,
            receipt,
            local,
            repo,
            wire,
            p,
            gateway,
        };
        if retire {
            h.sync()
                .await
                .context("DEPENDENCY actual Accepted N retirement whole sync")?;
            proof::settled(&h, &original).await?;
            let old = h.case.h.witness().await?;
            let receipt = h
                .gateway
                .decide_remove_member(h.case.h.state.input())
                .await
                .context("DEPENDENCY explicit packet-bearing Accepted N+1")?;
            let queued = h.case.h.witness().await?;
            ensure!(
                receipt.event_id == old.0.value.origin.as_ref().unwrap().event_id.clone().next()
                    && queued.0.value.previous_attempts == vec![compact(&old)?]
                    && queued.0.value.transport_noop.is_none(),
                "N+1 archive/source identity differs"
            );
            h.receipt = receipt;
            h.expect_current().await?;
        }
        let frozen = h
            .case
            .h
            .witness()
            .await?
            .0
            .value
            .payload
            .context("actual current FULL")?;
        h.sync()
            .await
            .context("DEPENDENCY actual Accepted current admission/publication sync")?;
        publication::common(&publication::measure(&h.case).await?)?;
        publication::published(&h.case, &frozen).await?;
        proof::published(&h, &frozen).await?;
        ensure!(
            h.repo.trace_valid(),
            "actual outer ALL-method trace/read order differs"
        );
        Ok(h)
    }
    pub async fn sync(&self) -> Result<()> {
        self.gateway.sync(self.case.h.state.b.user()).await
    }
    pub fn service(&self) -> MetaClientService<repo::Repo, wire::Forward> {
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
    async fn expect_current(&self) -> Result<()> {
        let witness = self.case.h.witness().await?;
        let frozen = witness.0.value.payload.context("actual N+1 FULL")?;
        let original_packets = self
            .local
            .inner
            .get_db()
            .await
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
            event_id: witness
                .0
                .value
                .origin
                .context("actual N+1 origin")?
                .event_id,
            witness_id: self.case.h.state.witness_id(),
            accepted: true,
            passes: frozen.passes.iter().map(|p| p.pass_id.clone()).collect(),
            original_packets,
            frozen,
        });
        self.case.observations.live.lock().unwrap().clear();
        self.case.observations.sent.lock().unwrap().clear();
        self.case.observations.uploads.lock().unwrap().clear();
        self.case.observations.noops.store(0, Ordering::SeqCst);
        Ok(())
    }
}
