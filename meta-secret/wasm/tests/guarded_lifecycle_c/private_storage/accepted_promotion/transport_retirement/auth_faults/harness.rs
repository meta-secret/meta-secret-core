use super::*;

pub(super) struct Harness {
    pub case: Case,
    pub repo: Arc<repo::Repo>,
    pub wire: Arc<wire::Forward>,
    pub gateway: SyncGateway<repo::Repo, wire::Forward>,
    pub original: LocalGuardedPreparationObject,
    pub origin: GuardedCommandOrigin,
    pub body: DeviceLogObject,
    pub request: RemoveMemberRequestEvent,
    pub canonical: (VaultObject, VaultLogObject),
    pub before: HashMap<ArtifactId, GenericKvLogEvent>,
}
pub(super) async fn create(mode: Mode) -> Result<Harness> {
    let (case, local) = case().await?;
    let before = local.inner.get_db().await;
    let (receipt, frozen) = case.prepare(true).await?;
    let original = case.h.witness().await?;
    private_crypto(&case.h.state, &original)?;
    let origin = original
        .0
        .value
        .origin
        .clone()
        .context("actual queued original origin")?;
    let body = local
        .inner
        .find_one(receipt.event_id.clone())
        .await?
        .context("actual queued original N")?
        .device_log()?;
    ensure!(
        original.0.value.payload.as_ref() == Some(&frozen)
            && origin.event_id == receipt.event_id
            && origin.body_commitment == producer_source_commitment(&body)?
            && original.0.value.transport_noop.is_none()
            && frozen.passes.len() == 2
            && case.observations.sent.lock().unwrap().is_empty(),
        "auth fault setup must be actual unsent FULL2 queued N"
    );
    local
        .capture_own_body(body.clone(), case.h.state.witness_id())
        .await?;
    let request = tombstone::delete_first(&case).await?;
    let canonical = canonical(&case).await?;
    let repo = Arc::new(repo::Repo::new(&case, &local, origin.clone(), mode));
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
        "outer adapter changed existing ownership guards"
    );
    let wire = Arc::new(wire::Forward::new(&case, repo.clone()));
    let gateway = SyncGateway::new(
        "actual-auth4-owned-prefix".into(),
        p,
        wire.clone(),
        case.h.state.master.clone(),
    );
    Ok(Harness {
        case,
        repo,
        wire,
        gateway,
        original,
        origin,
        body,
        request,
        canonical,
        before,
    })
}
