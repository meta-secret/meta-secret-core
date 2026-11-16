use super::*;
pub(super) struct Env {
    pub peer: prior::Peer,
    pub repo: Arc<observer::Repo>,
    pub wire: Arc<wire::Forward>,
    pub p: Arc<PersistentObject<observer::Repo>>,
    pub gateway: Arc<SyncGateway<observer::Repo, wire::Forward>>,
    pub service: Arc<MetaClientService<observer::Repo, wire::Forward>>,
    pub manager: ApplicationManager<observer::Repo, wire::Forward>,
}
impl Env {
    pub async fn wrap(peer: prior::Peer) -> Result<Self> {
        PersistentCredentials {
            p_obj: peer.p.clone(),
            master_key: peer.master.clone(),
        }
        .save_device_creds(peer.actor.device_creds.clone())
        .await?;
        peer.wire.stop_ss.store(false, Ordering::SeqCst);
        let repo = Arc::new(observer::Repo::new(&peer));
        let p = Arc::new(PersistentObject {
            repo: repo.clone(),
            device_log_append_guard: peer.p.device_log_append_guard.clone(),
            client_operation_guard: peer.p.client_operation_guard.clone(),
            device_sync_guard: peer.p.device_sync_guard.clone(),
        });
        ensure!(
            Arc::ptr_eq(&p.device_log_append_guard, &peer.p.device_log_append_guard)
                && Arc::ptr_eq(&p.client_operation_guard, &peer.p.client_operation_guard)
                && Arc::ptr_eq(&p.device_sync_guard, &peer.p.device_sync_guard),
            "SAME three owner Arcs changed"
        );
        *repo.guards.lock().unwrap() = Some((
            p.client_operation_guard.clone(),
            p.device_sync_guard.clone(),
        ));
        let wire = Arc::new(wire::Forward::new(&peer, repo.clone()));
        let gateway = Arc::new(SyncGateway::new(
            "actual-mobile-presync".into(),
            p.clone(),
            wire.clone(),
            peer.master.clone(),
        ));
        let service = Arc::new(MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer {
                dt: MetaClientRequestTransfer::new(),
            }),
            sync_gateway: gateway.clone(),
            state_provider: Arc::new(MetaClientStateProvider::new()),
            p_obj: p.clone(),
            device_data: peer.actor.device(),
            master_key: peer.master.clone(),
        });
        let before = peer.physical.get_db().await;
        let manager = ApplicationManager::new(
            wire.clone(),
            gateway.clone(),
            service.clone(),
            peer.master.clone(),
        );
        ensure!(
            peer.physical.get_db().await == before
                && Arc::ptr_eq(&manager.meta_client_service, &service)
                && Arc::ptr_eq(&manager.sync_gateway, &gateway)
                && Arc::ptr_eq(&service.p_obj, &p),
            "public manager constructor changed DB/actual wiring"
        );
        Ok(Self {
            peer,
            repo,
            wire,
            p,
            gateway,
            service,
            manager,
        })
    }
    pub fn reset(&self) {
        self.peer.trace_reset();
        self.repo.reset();
        self.wire.reset();
    }
}
pub(super) async fn before_prepare() -> Result<(Setup, prior::Data, Env)> {
    let inner = Arc::new(super::super::super::local_repo::Repo::new());
    let physical = inner.inner.clone();
    let case = Setup::new(inner).await?;
    let data = prior::Data {
        a: case.h.state.a.clone(),
        b: case.h.state.b.clone(),
        c: case.h.state.c.clone(),
        passes: case.h.state.passes.clone(),
        server: case.h.state.server.clone(),
        server_repo: case.h.state.server_repo.clone(),
    };
    let peer = prior::Peer::wrap(
        case.h.p.clone(),
        physical,
        data.b.clone(),
        case.h.state.master.clone(),
        data.server.clone(),
    )?;
    let e = Env::wrap(peer).await?;
    Ok((case, data, e))
}
pub(super) async fn stage_removed(
    case: &Setup,
    data: &prior::Data,
    e: &Env,
) -> Result<DeviceTransportNoop> {
    case.prepare(true).await?;
    super::super::super::tombstone::delete_first(case).await?;
    let b = Baseline::capture(&e.peer, &case.h.state.uuid).await?;
    e.peer
        .select(&case.h.state.uuid, Some(data.passes[0].clone()));
    // Prior presend oracle scopes attempts to this actual retirement, not healthy startup.
    e.peer.trace_reset();
    let auth = prior::presend(data, &e.peer, &b).await?;
    e.peer.wire.stop_ss.store(false, Ordering::SeqCst);
    remove_b(data).await?;
    ensure!(
        data.server_repo
            .find_one(auth.event_id.clone())
            .await?
            .is_none(),
        "actual server N already occupied"
    );
    e.reset();
    Ok(auth)
}
pub(super) async fn remove_b(data: &prior::Data) -> Result<()> {
    let ap = prior::Peer::from_server(data, data.a.clone()).await?;
    let cp = prior::Peer::from_server(data, data.c.clone()).await?;
    let (uuid, _) = data.request(&ap, &data.b).await?;
    data.approve(&cp, &uuid).await?;
    let (v, l) = data.canonical().await?;
    ensure!(
        v.0.value.members().len() == 2
            && !v.0.value.is_member(data.b.device_id())
            && matches!(v.0.value.find_user(data.b.device_id()),Some(UserMembership::Outsider(o)) if o.status==UserDataOutsiderStatus::Declined)
            && l.0
                .value
                .liveboard_removal_results
                .iter()
                .any(|r| r.request_id == uuid
                    && r.application_state == RemovalApplicationState::Applied),
        "actual RemoveB not Applied"
    );
    Ok(())
}
pub(super) async fn candidate(data: &prior::Data) -> Result<Env> {
    remove_b(data).await?;
    let mut actor = FixtureRegistry::empty().state.user_creds.client;
    actor.vault_name = data.b.vault_name.clone();
    ensure!(
        [data.a.device_id(), data.b.device_id(), data.c.device_id()]
            .into_iter()
            .all(|x| x != actor.device_id()),
        "candidate not fresh"
    );
    let raw = Arc::new(InMemKvLogEventRepo::default());
    let p = Arc::new(PersistentObject::new(raw.clone()));
    let master = TransportDsaKeyPair::generate().sk();
    let creds = PersistentCredentials {
        p_obj: p.clone(),
        master_key: master.clone(),
    };
    creds.save_device_creds(actor.device_creds.clone()).await?;
    creds.save_user_creds(actor.clone()).await?;
    let peer = prior::Peer::wrap(p, raw, actor.clone(), master, data.server.clone())?;
    let e = Env::wrap(peer).await?;
    e.service
        .handle_client_request(
            ApplicationState::Local(actor.device()),
            GenericAppStateRequest::SignUp(actor.vault_name.clone()),
        )
        .await?;
    let ap = prior::Peer::from_server(data, data.a.clone()).await?;
    ap.sync().await?;
    let log = PersistentVault::from(ap.p.clone())
        .get_vault_log_artifact(actor.vault_name.clone())
        .await?;
    let j = log
        .0
        .value
        .requests
        .iter()
        .find_map(|r| match r {
            VaultActionRequestEvent::JoinCluster(j) if j.candidate == actor.user() => Some(j),
            _ => None,
        })
        .context("real candidate Join absent")?;
    ap.service()
        .decide_join_request(CanonicalDecisionInput {
            request_id: j.request_id.clone(),
            accepted: false,
        })
        .await?;
    ap.sync().await?;
    e.reset();
    Ok(e)
}
