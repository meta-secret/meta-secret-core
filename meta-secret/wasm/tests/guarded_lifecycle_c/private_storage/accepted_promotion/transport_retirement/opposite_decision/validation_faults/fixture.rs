use super::*;
pub(super) struct Env<R: KvLogEventRepo> {
    pub h: OldHarness<R>,
    pub gate: Arc<observer::Gate<R>>,
    pub p: Arc<PersistentObject<observer::Gate<R>>>,
    pub gateway: Arc<SyncGateway<observer::Gate<R>, OldWire<R>>>,
}
impl<R: KvLogEventRepo> Env<R> {
    pub fn wrap(h: OldHarness<R>) -> Result<Self> {
        let gate = Arc::new(observer::Gate::new(&h));
        let p = Arc::new(PersistentObject {
            repo: gate.clone(),
            device_log_append_guard: h.p.device_log_append_guard.clone(),
            client_operation_guard: h.p.client_operation_guard.clone(),
            device_sync_guard: h.p.device_sync_guard.clone(),
        });
        ensure!(
            Arc::ptr_eq(&p.device_log_append_guard, &h.p.device_log_append_guard)
                && Arc::ptr_eq(&p.client_operation_guard, &h.p.client_operation_guard)
                && Arc::ptr_eq(&p.device_sync_guard, &h.p.device_sync_guard),
            "NEW fault observer changed SAME three public guards"
        );
        let gateway = Arc::new(SyncGateway::new(
            "actual-rollover-validation-faults".into(),
            p.clone(),
            h.wire.clone(),
            h.case.h.state.master.clone(),
        ));
        Ok(Self {
            h,
            gate,
            p,
            gateway,
        })
    }
    pub async fn decide(&self) -> Result<RemovalDecisionReceipt> {
        self.gateway
            .decide_remove_member(self.h.case.h.state.input())
            .await
    }
}
pub(super) async fn settled<R: KvLogEventRepo>(physical: Arc<R>) -> Result<(Env<R>, Retired)> {
    let h = OldHarness::fresh(physical).await?;
    let retired = super::super::proof::prerequisites(&h).await?;
    Ok((Env::wrap(h)?, retired))
}
pub(super) async fn queued<R: KvLogEventRepo>(physical: Arc<R>) -> Result<(Env<R>, Queued)> {
    let h = OldHarness::fresh(physical).await?;
    let retired = super::super::proof::prerequisites(&h).await?;
    let queued = super::super::proof::queue(&h, retired).await?;
    let e = Env::wrap(h)?;
    let retry = e.decide().await?;
    ensure!(
        retry.event_id == queued.event_id
            && digest(&e.h.case.h.witness().await?)? == queued.witness_hash
            && digest(&e.h.case.h.index().await?)? == queued.index_hash,
        "real healthy SAME-Accept retry failed before malformed fixture"
    );
    e.gate.reset();
    Ok((e, queued))
}
pub(super) async fn server_hash<R: KvLogEventRepo>(h: &OldHarness<R>) -> Result<String> {
    let mut entries = h
        .case
        .h
        .state
        .server_repo
        .get_db()
        .await
        .into_iter()
        .map(|(id, e)| Ok((digest(&id)?, digest(&e)?)))
        .collect::<Result<Vec<_>>>()?;
    entries.sort();
    digest(&entries)
}
