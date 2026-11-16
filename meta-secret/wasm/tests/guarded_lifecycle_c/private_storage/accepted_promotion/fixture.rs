use super::*;
use meta_secret_core::{
    crypto::key_pair::{KeyPair, TransportDsaKeyPair},
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        app::orchestrator::MetaOrchestrator,
        common::model::{
            user::common::{UserDataMember, UserMembership},
            vault::{vault::*, vault_data::VaultData},
        },
        db::{
            descriptors::vault_descriptor::*,
            events::{
                kv_log_event::*,
                vault::{
                    vault_event::VaultObject, vault_log_event::VaultLogObject,
                    vault_status::VaultStatusObject,
                },
            },
            repo::persistent_credentials::PersistentCredentials,
        },
        security::sign_event,
    },
};
use meta_server_node::server::server_app::ServerApp;

pub(super) struct Case<R: KvLogEventRepo> {
    pub h: H<repo::Observed<R>>,
    pub wire: Arc<wire::Forward<R>>,
    pub observations: Arc<Observations>,
}
impl<R: KvLogEventRepo> Case<R> {
    pub async fn new(inner: Arc<R>) -> Result<Self> {
        let observations = Arc::new(Observations::default());
        let h = H::new(Arc::new(repo::Observed::new(inner, observations.clone()))).await?;
        Ok(Self::from_h(h, observations))
    }
    pub async fn empty(inner: Arc<R>) -> Result<Self> {
        let observations = Arc::new(Observations::default());
        let observed = Arc::new(repo::Observed::new(inner, observations.clone()));
        let h = initial_empty(observed).await?;
        Ok(Self::from_h(h, observations))
    }
    pub fn from_h(h: H<repo::Observed<R>>, observations: Arc<Observations>) -> Self {
        *observations.owner.lock().unwrap() = Some(h.state.b.user_id());
        let wire = Arc::new(wire::Forward {
            local: h.p.repo.inner.clone(),
            server: h.state.server.clone(),
            observations: observations.clone(),
        });
        Self {
            h,
            wire,
            observations,
        }
    }
    pub fn gateway(&self) -> SyncGateway<ExistingRepo<repo::Observed<R>>, wire::Forward<R>> {
        SyncGateway::new(
            "C2-actual-accepted-promotion".into(),
            self.h.p.clone(),
            self.wire.clone(),
            self.h.state.master.clone(),
        )
    }
    pub async fn prepare(
        &self,
        accepted: bool,
    ) -> Result<(RemovalDecisionReceipt, GuardedPreparationPayload)> {
        let original_packets = self
            .h
            .p
            .repo
            .snapshot()
            .await?
            .into_values()
            .filter_map(|event| match event {
                GenericKvLogEvent::SsWorkflow(packet @ SsWorkflowObject::Distribution(_))
                    if packet
                        .clone()
                        .to_distribution_data()
                        .is_ok_and(|data| data.vault_name == self.h.state.b.vault_name) =>
                {
                    Some(packet)
                }
                _ => None,
            })
            .collect();
        let receipt = self
            .h
            .gateway()
            .decide_remove_member(CanonicalDecisionInput {
                request_id: self.h.state.uuid.clone(),
                accepted,
            })
            .await?;
        let witness = self.h.witness().await?;
        let frozen = witness
            .0
            .value
            .payload
            .clone()
            .context("actual complete stage")?;
        let origin = witness
            .0
            .value
            .origin
            .as_ref()
            .context("actual queued source")?;
        ensure!(
            origin.event_id == receipt.event_id && origin.accepted == accepted,
            "actual prepared origin/receipt differs"
        );
        *self.observations.expected.lock().unwrap() = Some(Expected {
            owner: self.h.state.b.user_id(),
            requester: self.h.state.a.device_id().clone(),
            target: self.h.state.c.device_id().clone(),
            uuid: self.h.state.uuid.clone(),
            event_id: receipt.event_id.clone(),
            witness_id: self.h.state.witness_id(),
            accepted,
            passes: self.h.state.passes.clone(),
            original_packets,
            frozen: frozen.clone(),
        });
        Ok((receipt, frozen))
    }
}

/// INITIAL empty three-member fixture; all subsequent admission/decision is real.
async fn initial_empty<R: KvLogEventRepo>(inner: Arc<R>) -> Result<H<R>> {
    let registry = FixtureRegistry::empty().state;
    let a = registry.user_creds.client;
    let b = registry.user_creds.client_b;
    let c = registry.user_creds.vd;
    let seed = Arc::new(PersistentObject::new(Arc::new(
        InMemKvLogEventRepo::default(),
    )));
    let mut vault = VaultData::from(UserDataMember::from(b.user()));
    for u in [&a, &c] {
        vault = vault.update_membership(UserMembership::Member(u.user().into()));
    }
    ensure!(
        vault.members().len() == 3 && vault.secrets.is_empty(),
        "initial empty Vault"
    );
    seed.repo
        .save(VaultObject(KvLogEvent {
            key: KvKey::from(VaultDescriptor::from(vault.vault_name.clone())),
            value: vault,
        }))
        .await?;
    seed.repo
        .save(VaultLogObject::create(b.user().into()))
        .await?;
    for u in [&a, &b, &c] {
        seed.repo
            .save(VaultStatusObject::new(
                VaultStatus::Member(u.user().into()),
                ArtifactId::from(VaultStatusDescriptor::from(u.user_id())),
            ))
            .await?;
    }
    let server_repo = Arc::new(InMemKvLogEventRepo::default());
    for event in seed.repo.get_db().await.into_values() {
        inner.save(event.clone()).await?;
        server_repo.save(event).await?;
    }
    let server = Arc::new(ServerApp::new(
        server_repo.clone(),
        registry.device_creds.server_master_key.clone(),
    )?);
    server.init().await?;
    let request = MetaOrchestrator {
        p_obj: seed.clone(),
        user_creds: a.clone(),
    }
    .request_remove_member(c.user().into())
    .await?;
    ensure!(
        request.share_transfers.is_empty(),
        "empty Vault generated fictitious transfers"
    );
    let body: meta_secret_core::node::db::events::vault::device_log_event::DeviceLogObject = seed
        .find_tail_event(DeviceLogDescriptor::from(a.user_id()))
        .await?
        .context("actual empty A request")?;
    server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            sign_event(
                body.to_generic(),
                a.device_id().clone(),
                &a.device_creds.key_manager()?.dsa,
            )?,
        ))))
        .await?
        .ensure_success()?;
    let p = Arc::new(PersistentObject::new(inner.clone()));
    let tail = PersistentVault::from(p.clone())
        .vault_tail(b.user())
        .await?;
    let response = server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            VaultRequest {
                sender: b.user(),
                tail,
            },
        ))))
        .await?
        .to_data()?;
    for event in response.0 {
        if inner.find_one(event.obj_id()).await?.is_some() {
            inner.replace(event).await?;
        } else {
            inner.save(event).await?;
        }
    }
    let log = PersistentVault::from(p)
        .get_vault_log_artifact(b.vault_name.clone())
        .await?;
    ensure!(
        log.0.value.liveboard_removal_pending.len() == 1,
        "real empty pending binding"
    );
    let binding = &log.0.value.liveboard_removal_pending[0];
    ensure!(
        binding.request == request,
        "empty fixture binding differs from real source"
    );
    let state = State {
        a,
        b,
        c,
        passes: vec![],
        uuid: binding.request_id.clone(),
        master: TransportDsaKeyPair::generate().sk(),
        server,
        server_repo,
        server_master: registry.device_creds.server_master_key,
    };
    let existing = Arc::new(ExistingRepo::new(
        inner,
        state.b.user_id(),
        state.uuid.clone(),
    ));
    // Initial records predate the observer wrapper; retain their real keys for snapshots.
    existing.remember(seed.repo.get_db().await.into_keys());
    existing.remember([log.obj_id()]);
    let h = H::open(existing, state)?;
    PersistentCredentials {
        p_obj: h.p.clone(),
        master_key: h.state.master.clone(),
    }
    .save_user_creds(h.state.b.clone())
    .await?;
    Ok(h)
}
