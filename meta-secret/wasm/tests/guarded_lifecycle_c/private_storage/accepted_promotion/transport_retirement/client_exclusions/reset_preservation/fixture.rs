use super::*;

pub(super) struct Env {
    pub peer: prior::Peer,
    pub repo: Arc<observer::Repo>,
    pub wire: Arc<wire::Forward>,
    pub p: Arc<PersistentObject<observer::Repo>>,
    pub gateway: Arc<SyncGateway<observer::Repo, wire::Forward>>,
    pub service: Arc<MetaClientService<observer::Repo, wire::Forward>>,
}
impl Env {
    pub async fn wrap(peer: prior::Peer) -> Result<Self> {
        // Actual production credential persistence, before the measured scenario.
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
            "Reset adapter changed SAME three guards"
        );
        *repo.guards.lock().unwrap() = Some((
            p.client_operation_guard.clone(),
            p.device_sync_guard.clone(),
        ));
        let wire = Arc::new(wire::Forward::new(&peer, repo.clone()));
        let gateway = Arc::new(SyncGateway::new(
            "actual-Reset-preservation".into(),
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
        let e = Self {
            peer,
            repo,
            wire,
            p,
            gateway,
            service,
        };
        e.reset();
        Ok(e)
    }
    pub fn reset(&self) {
        self.peer.trace_reset();
        self.repo.reset();
        self.wire.reset();
    }
}
pub(super) async fn remove_b(data: &prior::Data) -> Result<String> {
    let ap = prior::Peer::from_server(data, data.a.clone()).await?;
    let cp = prior::Peer::from_server(data, data.c.clone()).await?;
    remove_b_with(data, &ap, &cp).await
}
pub(super) async fn remove_b_with(
    data: &prior::Data,
    ap: &prior::Peer,
    cp: &prior::Peer,
) -> Result<String> {
    let (uuid, _) = data.request(ap, &data.b).await?;
    data.approve(cp, &uuid).await?;
    let (v, l) = data.canonical().await?;
    ensure!(
        !v.0.value.is_member(data.b.device_id())
            && v.0.value.members().len() == 2
            && v.0.value.is_member(data.a.device_id())
            && v.0.value.is_member(data.c.device_id())
            && matches!(v.0.value.find_user(data.b.device_id()),Some(UserMembership::Outsider(o))
            if o.status==UserDataOutsiderStatus::Declined)
            && l.0
                .value
                .liveboard_removal_results
                .iter()
                .any(|r| r.request_id == uuid
                    && r.application_state == RemovalApplicationState::Applied),
        "DEPENDENCY real A request/C eligible approval did not remove B"
    );
    eprintln!(
        "RESET_PRESERVATION real_A_RemoveB=true real_C_approval=true canonical_Applied=true B_Outsider_Declined=true"
    );
    Ok(uuid)
}
pub(super) async fn removed_noop() -> Result<(prior::Data, Env, DeviceTransportNoop)> {
    let (data, p, b) = prior::retirement().await?;
    let auth = prior::presend(&data, &p, &b).await?;
    remove_b(&data).await?;
    let e = Env::wrap(p).await?;
    ensure!(
        data.server_repo
            .find_one(auth.event_id.clone())
            .await?
            .is_none(),
        "old transport slot not empty before actual GetState"
    );
    Ok((data, e, auth))
}
pub(super) async fn removed_plain() -> Result<(prior::Data, Env)> {
    let (data, _ap, bp) = prior::initial().await?;
    remove_b(&data).await?;
    let e = Env::wrap(bp).await?;
    ensure!(
        !e.peer
            .physical
            .get_db()
            .await
            .values()
            .any(|v| matches!(v,GenericKvLogEvent::DeviceLog(b)
        if b.obj_id().fqdn==ArtifactId::from(DeviceLogDescriptor::from(data.b.user_id())).fqdn)),
        "owner fixture B has outstanding Business outbox"
    );
    Ok((data, e))
}
pub(super) async fn recovery(
    data: &prior::Data,
    actor: &prior::Peer,
    receiver: &Env,
    pass: &MetaPasswordId,
) -> Result<SsClaim> {
    let receipt = actor.service().recover_checked(pass.clone()).await?;
    receiver.gateway.sync(receiver.peer.actor.user()).await?;
    let raw = PersistentSharedSecret::from(Arc::new(PersistentObject::new(
        receiver.peer.physical.clone(),
    )))
    .get_ss_log_obj(data.b.vault_name.clone())
    .await?;
    let claims = raw
        .claims
        .values()
        .filter(|c| receipt.claim_id.as_deref() == Some(c.id.wasm_id_str().as_str()))
        .collect::<Vec<_>>();
    let [c] = claims.as_slice() else {
        anyhow::bail!("DEPENDENCY actual checked Recovery not admitted/imported")
    };
    ensure!(
        c.sender == *actor.actor.device_id()
            && c.distribution_type == SecretDistributionType::Recover
            && c.dist_claim_id.pass_id == *pass
            && c.receivers.len() == 2
            && c.receivers.contains(receiver.peer.actor.device_id()),
        "actual Recovery actor/Pass/receivers differ"
    );
    Ok((*c).clone())
}
pub(super) async fn candidate(data: &prior::Data) -> Result<Env> {
    // Real removal frees one slot before a genuinely new declined Join candidate.
    remove_b(data).await?;
    let mut actor = FixtureRegistry::empty().state.user_creds.client;
    actor.vault_name = data.b.vault_name.clone();
    ensure!(
        [data.a.device_id(), data.b.device_id(), data.c.device_id()]
            .iter()
            .all(|x| **x != *actor.device_id()),
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
    let state = e
        .service
        .handle_client_request(
            ApplicationState::Local(actor.device()),
            GenericAppStateRequest::SignUp(actor.vault_name.clone()),
        )
        .await?;
    ensure!(
        matches!(state,ApplicationState::Vault(VaultFullInfo::Outsider(o)) if o.status==UserDataOutsiderStatus::Pending),
        "DEPENDENCY real candidate Join not Pending"
    );
    let ap = prior::Peer::from_server(data, data.a.clone()).await?;
    ap.sync().await?;
    let log = PersistentVault::from(ap.p.clone())
        .get_vault_log_artifact(actor.vault_name.clone())
        .await?;
    let joins = log
        .0
        .value
        .requests
        .iter()
        .filter_map(|r| match r {
            VaultActionRequestEvent::JoinCluster(j) if j.candidate == actor.user() => {
                Some(j.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [join] = joins.as_slice() else {
        anyhow::bail!("DEPENDENCY actual candidate Join missing")
    };
    ap.service()
        .decide_join_request(CanonicalDecisionInput {
            request_id: join.request_id.clone(),
            accepted: false,
        })
        .await?;
    ap.sync().await?;
    e.reset();
    Ok(e)
}
