use super::*;

pub(super) struct Peer {
    pub actor: UserCreds,
    pub master: TransportSk,
    pub physical: Arc<InMemKvLogEventRepo>,
    pub repo: Arc<observer::Repo>,
    pub p: Arc<PersistentObject<observer::Repo>>,
    pub wire: Arc<wire::Forward>,
    pub gateway: Arc<SyncGateway<observer::Repo, wire::Forward>>,
}
impl Peer {
    pub fn wrap<R: KvLogEventRepo>(
        original: Arc<PersistentObject<R>>,
        physical: Arc<InMemKvLogEventRepo>,
        actor: UserCreds,
        master: TransportSk,
        server: Arc<ServerApp<InMemKvLogEventRepo>>,
    ) -> Result<Self> {
        let repo = Arc::new(observer::Repo::new(physical.clone()));
        let p = Arc::new(PersistentObject {
            repo: repo.clone(),
            device_log_append_guard: original.device_log_append_guard.clone(),
            client_operation_guard: original.client_operation_guard.clone(),
            device_sync_guard: original.device_sync_guard.clone(),
        });
        ensure!(
            Arc::ptr_eq(
                &p.device_log_append_guard,
                &original.device_log_append_guard
            ) && Arc::ptr_eq(&p.client_operation_guard, &original.client_operation_guard)
                && Arc::ptr_eq(&p.device_sync_guard, &original.device_sync_guard),
            "SAME three owner Arcs changed"
        );
        let wire = Arc::new(wire::Forward::new(server, repo.clone(), actor.user_id()));
        let gateway = Arc::new(SyncGateway::new(
            "actual-client-exclusions".into(),
            p.clone(),
            wire.clone(),
            master.clone(),
        ));
        Ok(Self {
            actor,
            master,
            physical,
            repo,
            p,
            wire,
            gateway,
        })
    }
    pub async fn from_server(data: &Data, actor: UserCreds) -> Result<Self> {
        let physical = Arc::new(InMemKvLogEventRepo::default());
        // Initial peer bootstrap retains ACTUAL canonical bytes and existing own source prefix.
        // Every subsequent topology/request transition uses real production calls and ServerApp.
        for event in data.server_repo.get_db().await.into_values() {
            let event = match event {
                GenericKvLogEvent::UserCreds(_)
                | GenericKvLogEvent::DeviceCreds(_)
                | GenericKvLogEvent::LocalGuardedPreparation(_)
                | GenericKvLogEvent::LocalGuardedPreparationIndex(_)
                | GenericKvLogEvent::LocalRemovalEnvelope(_) => continue,
                GenericKvLogEvent::VaultLog(mut l) => {
                    l.0.value = l.0.value.without_private_producer_metadata();
                    l.to_generic()
                }
                other => other,
            };
            physical.save(event).await?;
        }
        let original = Arc::new(PersistentObject::new(physical.clone()));
        let master = TransportDsaKeyPair::generate().sk();
        PersistentCredentials {
            p_obj: original.clone(),
            master_key: master.clone(),
        }
        .save_user_creds(actor.clone())
        .await?;
        Self::wrap(original, physical, actor, master, data.server.clone())
    }
    pub fn service(&self) -> MetaClientService<observer::Repo, wire::Forward> {
        MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer {
                dt: MetaClientRequestTransfer::new(),
            }),
            sync_gateway: self.gateway.clone(),
            state_provider: Arc::new(MetaClientStateProvider::new()),
            p_obj: self.p.clone(),
            device_data: self.actor.device(),
            master_key: self.master.clone(),
        }
    }
    pub async fn sync(&self) -> Result<()> {
        self.gateway.sync(self.actor.user()).await
    }
    pub async fn witness(&self, uuid: &str) -> Result<LocalGuardedPreparationObject> {
        self.physical
            .find_one(self.witness_id(uuid))
            .await?
            .context("actual physical witness")?
            .try_into()
    }
    pub fn witness_id(&self, uuid: &str) -> ArtifactId {
        ArtifactId::from(LocalGuardedPreparationDescriptor {
            owner: self.actor.user_id(),
            request_id: uuid.into(),
        })
    }
    pub async fn registry(&self) -> Result<GenericKvLogEvent> {
        let r = PersistentRemovalEnvelope {
            p_obj: self.p.clone(),
        }
        .load(&self.actor.user_id())
        .await?
        .context("actual registry tail")?;
        Ok(r.to_generic())
    }
    pub fn trace_reset(&self) {
        self.repo.reset();
        self.wire.reset();
    }
    pub fn select(&self, uuid: &str, deleted: Option<MetaPasswordId>) {
        *self.wire.selected.lock().unwrap() = Some((self.witness_id(uuid), deleted));
        self.wire.stop_ss.store(true, Ordering::SeqCst);
    }
}
pub(super) struct Data {
    pub a: UserCreds,
    pub b: UserCreds,
    pub c: UserCreds,
    pub passes: Vec<MetaPasswordId>,
    pub server: Arc<ServerApp<InMemKvLogEventRepo>>,
    pub server_repo: Arc<InMemKvLogEventRepo>,
}
impl Data {
    pub async fn canonical(&self) -> Result<(VaultObject, VaultLogObject)> {
        let p = PersistentVault::from(Arc::new(PersistentObject::new(self.server_repo.clone())));
        Ok((
            p.get_vault(self.b.vault_name.clone()).await?,
            p.get_vault_log_artifact(self.b.vault_name.clone()).await?,
        ))
    }
    pub async fn hash(&self) -> Result<String> {
        digest(&self.canonical().await?)
    }
    pub async fn tail(&self, actor: &UserCreds) -> Result<Option<ArtifactId>> {
        Ok(self
            .server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
                ServerTailRequest {
                    sender: actor.user(),
                    producer_reference: None,
                },
            ))))
            .await?
            .to_server_tail()?
            .device_log_tail)
    }
    pub async fn request(
        &self,
        actor: &Peer,
        target: &UserCreds,
    ) -> Result<(String, DeviceLogObject)> {
        let receipt = actor
            .service()
            .remove_member_checked(target.device_id().to_string())
            .await?;
        let id = receipt.request_stage.source.event_id;
        let body = actor
            .physical
            .find_one(id.clone())
            .await?
            .context("actual public producer body")?
            .device_log()?;
        ensure!(
            matches!(&body.0.value, VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r))
            if r.sender.user_data == actor.actor.user() && r.target.user_data == target.user()),
            "actual public Remove producer differs"
        );
        let (_, log) = self.canonical().await?;
        let bindings = log.0.value.liveboard_removal_pending.iter().filter(|b|
            matches!(&body.0.value, VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r)) if b.request == *r))
            .collect::<Vec<_>>();
        let [binding] = bindings.as_slice() else {
            anyhow::bail!("DEPENDENCY actual public Remove admission missing")
        };
        let next = self
            .tail(&actor.actor)
            .await?
            .context("actual admitted request next tail")?;
        ensure!(
            next.fqdn == id.fqdn && next.id.curr > id.id.curr,
            "DEPENDENCY public request lacks actual ordered server admission"
        );
        ensure!(
            self.server_repo.find_one(id).await?.is_some(),
            "DEPENDENCY actual request raw source absent"
        );
        Ok((binding.request_id.clone(), body))
    }
    pub async fn approve(&self, actor: &Peer, uuid: &str) -> Result<DeviceLogObject> {
        actor.sync().await?;
        let receipt = actor
            .gateway
            .decide_remove_member(CanonicalDecisionInput {
                request_id: uuid.into(),
                accepted: true,
            })
            .await?;
        let body = actor
            .physical
            .find_one(receipt.event_id.clone())
            .await?
            .context("actual checked approval source")?
            .device_log()?;
        ensure!(
            self.server_repo
                .find_one(receipt.event_id.clone())
                .await?
                .is_none(),
            "actual approval source already occupied"
        );
        actor.sync().await?;
        let (_, log) = self.canonical().await?;
        let result = log
            .0
            .value
            .liveboard_removal_results
            .iter()
            .find(|r| r.request_id == uuid)
            .context("DEPENDENCY real approval not canonically Applied")?;
        let source = result
            .source
            .as_ref()
            .context("DEPENDENCY canonical Applied lacks actual source")?;
        ensure!(
            result.application_state == RemovalApplicationState::Applied
                && source.event_id == receipt.event_id
                && source.signer == *actor.actor.device_id(),
            "DEPENDENCY canonical winner not actual approver/source"
        );
        Ok(body)
    }
}

pub(super) async fn retirement() -> Result<(Data, Peer, Baseline)> {
    let local = Arc::new(local_repo::Repo::new());
    let physical = local.inner.clone();
    let case = super::super::super::fixture::Case::new(local).await?;
    case.prepare(true).await?;
    super::super::tombstone::delete_first(&case).await?;
    let data = Data {
        a: case.h.state.a.clone(),
        b: case.h.state.b.clone(),
        c: case.h.state.c.clone(),
        passes: case.h.state.passes.clone(),
        server: case.h.state.server.clone(),
        server_repo: case.h.state.server_repo.clone(),
    };
    let peer = Peer::wrap(
        case.h.p.clone(),
        physical,
        data.b.clone(),
        case.h.state.master.clone(),
        data.server.clone(),
    )?;
    let baseline = Baseline::capture(&peer, &case.h.state.uuid).await?;
    ensure!(
        data.server_repo
            .find_one(baseline.origin.event_id.clone())
            .await?
            .is_none(),
        "actual N was sent before exclusion"
    );
    peer.select(&baseline.origin.request_id, Some(data.passes[0].clone()));
    peer.trace_reset();
    Ok((data, peer, baseline))
}

pub(super) async fn initial() -> Result<(Data, Peer, Peer)> {
    let f = FixtureRegistry::empty().state;
    let a = f.user_creds.client;
    let b = f.user_creds.client_b;
    let c = f.user_creds.vd;
    let seed = Arc::new(PersistentObject::new(Arc::new(
        InMemKvLogEventRepo::default(),
    )));
    let mut vault = VaultData::from(UserDataMember::from(b.user()));
    for actor in [&a, &c] {
        vault = vault.update_membership(UserMembership::Member(actor.user().into()));
    }
    seed.repo
        .save(VaultLogObject::create(b.user().into()))
        .await?;
    for actor in [&a, &b, &c] {
        seed.repo
            .save(VaultStatusObject::new(
                VaultStatus::Member(actor.user().into()),
                ArtifactId::from(VaultStatusDescriptor::from(actor.user_id())),
            ))
            .await?;
    }
    let mut passes = vec![];
    for n in 0..2 {
        let pass = PlainPassInfo::new(
            format!("client-exclusion-{n}"),
            "test-only-initial-value".into(),
        );
        passes.push(pass.pass_id.clone());
        vault = vault.add_secret(pass.pass_id.clone());
        let object = VaultObject(KvLogEvent {
            key: KvKey::from(VaultDescriptor::from(vault.vault_name.clone())),
            value: vault.clone(),
        });
        if seed.repo.find_one(object.obj_id()).await?.is_some() {
            seed.repo.replace(object).await?;
        } else {
            seed.repo.save(object).await?;
        }
        let member = VaultMember {
            member: a.user().into(),
            vault: vault.clone(),
        };
        MetaDistributor {
            p_obj: seed.clone(),
            user_creds: Arc::new(a.clone()),
            vault_member: member.clone(),
        }
        .distribute(member, SecurePassInfo::from(pass))
        .await?;
        let shared = PersistentSharedSecret::from(seed.clone());
        let claim = shared
            .find_ss_device_log_tail_event(a.device_id().clone())
            .await?
            .context("actual initial Split")?
            .to_distribution_request();
        shared.save_ss_log_event(claim).await?; // Initial material, not later protocol evidence.
    }
    let server_repo = Arc::new(InMemKvLogEventRepo::default());
    for event in seed.repo.get_db().await.into_values() {
        server_repo.save(event).await?;
    }
    let server = Arc::new(ServerApp::new(
        server_repo.clone(),
        f.device_creds.server_master_key,
    )?);
    server.init().await?;
    let data = Data {
        a,
        b,
        c,
        passes,
        server,
        server_repo,
    };
    ensure!(
        data.canonical()
            .await?
            .1
            .0
            .value
            .liveboard_removal_pending
            .is_empty(),
        "initial fixture already contains Remove"
    );
    let ap = Peer::from_server(&data, data.a.clone()).await?;
    let bp = Peer::from_server(&data, data.b.clone()).await?;
    Ok((data, ap, bp))
}

pub(super) async fn presend(data: &Data, p: &Peer, b: &Baseline) -> Result<DeviceTransportNoop> {
    p.wire.stop_noop.store(true, Ordering::SeqCst);
    let r = p.sync().await;
    ensure!(
        r.as_ref().err().is_some_and(|e| e.is::<wire::PreNoop>()),
        "DEPENDENCY real auth/purge pre-Noop boundary not reached"
    );
    let w = p.witness(&b.origin.request_id).await?;
    let n =
        w.0.value
            .transport_noop
            .as_ref()
            .context("actual durable auth before send")?;
    ensure!(
        w.0.value.payload.is_none()
            && n.receipt.is_none()
            && n.confirmed_next_tail.is_none()
            && checks::ordinary_clean(&p.physical.get_db().await, &data.passes[0])
            && data
                .server_repo
                .find_one(b.origin.event_id.clone())
                .await?
                .is_none(),
        "actual pre-send privacy/empty slot missing"
    );
    {
        let calls = p.wire.noops.lock().unwrap();
        ensure!(
            calls.len() == 1
                && !calls[0].forwarded
                && calls[0].authorization == n.authorization
                && calls[0].durable
                && calls[0].readback
                && calls[0].private_clean
                && calls[0].ordinary_clean,
            "actual before-Noop stop lacks durable production readback/privacy proof"
        );
        ensure!(
            p.wire.actions.lock().unwrap().is_empty(),
            "Business sent before technical boundary"
        );
    }
    b.binding(p, true).await?;
    let auth = n.authorization.clone();
    auth.verify(&p.actor.user().device.keys.dsa_pk)?;
    b.authenticated_cleanup(p, &auth)?;
    p.wire.stop_noop.store(false, Ordering::SeqCst);
    p.trace_reset();
    eprintln!(
        "CLIENT_EXCLUSIONS real_presend=true durable_auth=true outer_readback=true private_ordinary_purged=true original_slot_empty=true drained=true"
    );
    Ok(auth)
}

pub(super) async fn settled(data: &Data, p: &Peer, b: &Baseline, r: &Result<()>) -> Result<()> {
    ensure!(
        p.wire.stopped_ss(r),
        "SEMANTIC retirement did not reach exact FIRST SS boundary"
    );
    let w = p.witness(&b.origin.request_id).await?;
    let n =
        w.0.value
            .transport_noop
            .as_ref()
            .context("SEMANTIC actual retirement auth absent")?;
    let receipt = n
        .receipt
        .as_ref()
        .context("SEMANTIC actual receipt absent")?;
    ensure!(
        w.0.value.payload.is_none()
            && n.authorization.event_id == b.origin.event_id
            && n.authorization.client_entry_commitment == b.origin.body_commitment
            && receipt.authorization_hash == n.authorization.authorization_hash()?
            && receipt.record_effect == TransportNoopEffect::NotApplied
            && receipt.transport_state == TransportNoopState::NoopCommitted
            && n.confirmed_next_tail == Some(b.origin.event_id.clone().next()),
        "SEMANTIC retirement identity/receipt/tail/privacy differs"
    );
    n.authorization.verify(&p.actor.user().device.keys.dsa_pk)?;
    let raw = data
        .server_repo
        .find_one(b.origin.event_id.clone())
        .await?
        .context("actual server marker")?;
    let GenericKvLogEvent::DeviceTransportNoop(marker) = raw else {
        anyhow::bail!("actual N not Noop")
    };
    marker.validate(&p.actor.user_id(), &p.actor.user().device.keys.dsa_pk)?;
    ensure!(
        marker.0.value.authorization == n.authorization && marker.receipt() == *receipt,
        "actual marker differs"
    );
    {
        let cuts = p.wire.cuts.lock().unwrap();
        let first = &cuts[0];
        ensure!(
            first.1 == p.actor.user_id()
                && first.2.fqdn
                    == ArtifactId::from(SsLogDescriptor::from(p.actor.vault_name.clone())).fqdn,
            "wrong SS boundary actor/stream"
        );
        let calls = p.wire.noops.lock().unwrap();
        let call = calls
            .iter()
            .find(|c| c.receipt.as_ref() == Some(receipt))
            .context("actual returned receipt")?;
        ensure!(call.durable && call.readback && call.private_clean && call.ordinary_clean && call.forwarded
            && p.repo.reads.lock().unwrap().iter().any(|read| read.order > call.order && read.order < first.0
                && matches!(&read.event, Some(GenericKvLogEvent::LocalGuardedPreparation(x))
                    if x.0.value.transport_noop.as_ref().is_some_and(|t| t.receipt.as_ref() == Some(receipt))))
            && p.wire.tails.lock().unwrap().iter().any(|t| t.order > call.order && t.order < first.0
                && t.response.device_log_tail == Some(b.origin.event_id.clone().next()))
            && p.repo.exact_read_before(&w.clone().to_generic(), first.0), "actual receipt/separate Tail/readback did not precede FIRST SS");
    }
    ensure!(
        p.wire.actions.lock().unwrap().is_empty(),
        "technical proof received Business ACK"
    );
    b.authenticated_cleanup(p, &n.authorization)?;
    b.binding(p, true).await
}
