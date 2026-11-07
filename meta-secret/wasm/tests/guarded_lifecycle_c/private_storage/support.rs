use super::repo::Repo;
use super::*;
use meta_secret_core::{
    crypto::{
        key_pair::{KeyPair, TransportDsaKeyPair},
        keys::TransportSk,
    },
    node::{
        common::model::{meta_pass::MetaPasswordId, user::user_creds::UserCreds},
        db::repo::persistent_credentials::PersistentCredentials,
    },
};
use meta_server_node::server::server_app::ServerApp;
use std::sync::atomic::AtomicUsize;

pub struct State {
    pub a: UserCreds,
    pub b: UserCreds,
    pub c: UserCreds,
    pub passes: Vec<MetaPasswordId>,
    pub uuid: String,
    pub master: TransportSk,
    pub server: Arc<ServerApp<InMemKvLogEventRepo>>,
    pub server_repo: Arc<InMemKvLogEventRepo>,
    pub server_master: TransportSk,
}
impl State {
    pub fn input(&self) -> CanonicalDecisionInput {
        CanonicalDecisionInput {
            request_id: self.uuid.clone(),
            accepted: true,
        }
    }
    pub fn index_id(&self) -> ArtifactId {
        ArtifactId::from(LocalGuardedPreparationIndexDescriptor {
            owner: self.b.user_id(),
        })
    }
    pub fn witness_id(&self) -> ArtifactId {
        ArtifactId::from(LocalGuardedPreparationDescriptor {
            owner: self.b.user_id(),
            request_id: self.uuid.clone(),
        })
    }
}
pub struct Wire {
    pub server: Arc<ServerApp<InMemKvLogEventRepo>>,
    pub writes: AtomicUsize,
}
impl SyncProtocol for Wire {
    async fn send(&self, r: SyncRequest) -> Result<DataSyncResponse> {
        if matches!(r, SyncRequest::Write(_)) {
            self.writes.fetch_add(1, Ordering::SeqCst);
        }
        Box::pin(self.server.handle_client_request(r)).await
    }
}
pub struct H<R: KvLogEventRepo> {
    pub p: Arc<PersistentObject<Repo<R>>>,
    pub state: State,
    pub wire: Arc<Wire>,
}
impl<R: KvLogEventRepo> H<R> {
    pub async fn new(inner: Arc<R>) -> Result<Self> {
        let seed = super::super::fixture::Harness::new().await?;
        let state = State {
            a: seed.a.clone(),
            b: seed.b.clone(),
            c: seed.c.clone(),
            passes: seed.passes.clone(),
            uuid: seed.request_id.clone(),
            master: TransportDsaKeyPair::generate().sk(),
            server: seed.network.server.clone(),
            server_repo: seed.server_repo.clone(),
            server_master: seed.server_master.clone(),
        };
        let repo = Arc::new(Repo::new(inner, state.b.user_id(), state.uuid.clone()));
        for event in seed.p.repo.get_db().await.into_values() {
            if !matches!(
                event,
                GenericKvLogEvent::UserCreds(_) | GenericKvLogEvent::DeviceCreds(_)
            ) {
                repo.save(event).await?;
            }
        }
        let h = Self::open(repo, state)?;
        PersistentCredentials {
            p_obj: h.p.clone(),
            master_key: h.state.master.clone(),
        }
        .save_user_creds(h.state.b.clone())
        .await?;
        h.p.repo.trace.lock().unwrap().clear();
        h.p.repo.before_body.lock().unwrap().clear();
        Ok(h)
    }
    pub fn open(repo: Arc<Repo<R>>, state: State) -> Result<Self> {
        let wire = Arc::new(Wire {
            server: state.server.clone(),
            writes: 0.into(),
        });
        Ok(Self {
            p: Arc::new(PersistentObject::new(repo)),
            state,
            wire,
        })
    }
    pub fn gateway(&self) -> SyncGateway<Repo<R>, Wire> {
        SyncGateway::new(
            "actual-C1b".into(),
            self.p.clone(),
            self.wire.clone(),
            self.state.master.clone(),
        )
    }
    pub async fn witness(&self) -> Result<LocalGuardedPreparationObject> {
        self.p
            .repo
            .inner
            .find_one(self.state.witness_id())
            .await?
            .context("required real fixed private witness absent (late faults UNREACHED)")?
            .try_into()
    }
    pub async fn index(&self) -> Result<LocalGuardedPreparationIndexObject> {
        self.p
            .repo
            .inner
            .find_one(self.state.index_id())
            .await?
            .context("required real fixed preparation index absent")?
            .try_into()
    }
    pub async fn decide(&self) -> Result<RemovalDecisionReceipt> {
        self.gateway()
            .decide_remove_member(self.state.input())
            .await
    }
    pub async fn no_body_or_send(&self) -> Result<()> {
        let events = self.p.repo.snapshot().await?;
        let own_stream = ArtifactId::from(
            meta_secret_core::node::db::descriptors::vault_descriptor::DeviceLogDescriptor::from(
                self.state.b.user_id(),
            ),
        )
        .fqdn;
        ensure!(
            !events
                .values()
                .any(|e| matches!(e, GenericKvLogEvent::DeviceLog(body) if body.obj_id().fqdn==own_stream)),
            "private failure left an actual queued body"
        );
        ensure!(
            self.wire.writes.load(Ordering::SeqCst) == 0,
            "private failure sent a command"
        );
        Ok(())
    }
}
pub fn ordinary(s: &super::super::fixture::Snapshot) -> super::super::fixture::Snapshot {
    super::super::fixture::ordinary(s)
}
pub fn static_error<T>(r: &Result<T>) -> Result<()> {
    let e = r
        .as_ref()
        .err()
        .context("private failure unexpectedly returned queued receipt")?
        .to_string();
    ensure!(
        !e.contains("PRIVATE_SENTINEL"),
        "private backend content leaked through public error"
    );
    Ok(())
}
