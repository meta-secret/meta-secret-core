use super::super::super::{GLOBAL_APP_MANAGER, MobileApplicationManager};
use super::{http::Http, seed_fixture};
use crate::app_manager::ApplicationManager;
use anyhow::{Context, Result, ensure};
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};
use meta_secret_core::node::{
    app::{
        meta_app::{
            client_transfer::MetaClientRequestTransfer,
            meta_client_service::{
                MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
            },
        },
        sync::{api_url::ApiUrl, sync_gateway::SyncGateway, sync_protocol::HttpSyncProtocol},
    },
    common::model::{
        meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo},
        secret::{SsClaim, SsDistributionStatus},
        user::{common::UserDataMember, user_creds::UserCreds},
        vault::vault::VaultMember,
    },
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::{
            generic_log_event::*,
            object_id::Next,
            shared_secret_event::SsWorkflowObject,
            vault::{
                device_log_event::DeviceLogObject, liveboard_metadata::RecoveryDecisionRecord,
            },
        },
        in_mem_db::InMemKvLogEventRepo,
        objects::{
            persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
            persistent_vault::PersistentVault,
        },
        repo::generic_db::*,
    },
};
use meta_secret_core::secret::MetaDistributor;
use meta_server_node::server::server_app::ServerApp;
use serde_json::Value;

pub const OTHER_VALUE: &str = "native-second-controlled-value";
pub enum CanonicalDelta {
    ApproveOther,
    VaultRevision,
}
pub struct Parallel {
    pub pass: MetaPasswordId,
    pub claim: SsClaim,
    packet: SsWorkflowObject,
    result: RecoveryDecisionRecord,
}
use std::{sync::Arc, thread::JoinHandle};

pub struct Harness {
    _dir: tempfile::TempDir,
    pub p: Arc<PersistentObject<SqlIteRepo>>,
    pub actor: UserCreds,
    pub initiator: UserCreds,
    pub target: UserCreds,
    pub secret: MetaPasswordId,
    pub recovery: Option<SsClaim>,
    pub parallel: Option<Parallel>,
    pub server_repo: Arc<InMemKvLogEventRepo>,
    pub http: Http,
    previous: Option<Arc<MobileApplicationManager>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}
impl Harness {
    pub fn new() -> Result<Self> {
        Self::new_seeded(false)
    }
    pub fn new_recovery() -> Result<Self> {
        Self::new_seeded(true)
    }
    fn new_seeded(recovery: bool) -> Result<Self> {
        Self::new_config(recovery, None)
    }
    pub fn new_parallel(delayed: bool) -> Result<Self> {
        Self::new_config(true, Some(delayed))
    }
    pub fn new_registration_capacity(members: usize) -> Result<Self> {
        ensure!(matches!(members, 2 | 3), "REGISTRATION_MEMBER_COUNT");
        Self::new_config_with_members(false, None, members)
    }
    fn new_config(recovery: bool, delayed: Option<bool>) -> Result<Self> {
        Self::new_config_with_members(recovery, delayed, 3)
    }
    fn new_config_with_members(
        recovery: bool,
        delayed: Option<bool>,
        members: usize,
    ) -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let filename = if recovery {
            "native-recovered-show.sqlite"
        } else {
            "native-producer.sqlite"
        };
        let url = format!("file:{}", dir.path().join(filename).display());
        EmbeddedMigrationsTool {
            db_url: url.clone(),
        }
        .migrate();
        let p = Arc::new(PersistentObject::new(Arc::new(SqlIteRepo {
            conn_url: url,
        })));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (seed, secret, recovery, parallel, server_repo, server) = runtime.block_on(async {
            let mut seed = seed_fixture::Harness::new(members).await?;
            let secret = seed.seed_secret("native-exact-producer").await?;
            let (recovery, parallel) = if let Some(delayed) = delayed {
                let other = seed_other(&mut seed).await?;
                let (first, second) = seed_parallel(&seed, &secret, other, delayed).await?;
                (Some(first), Some(second))
            } else if recovery {
                (
                    Some(super::recovered_show::seed_accepted(&seed, &secret).await?),
                    None,
                )
            } else {
                (None, None)
            };
            let server_repo = Arc::new(InMemKvLogEventRepo::default());
            for value in seed.p.repo.get_db().await.into_values() {
                p.repo.save(value.clone()).await?;
                if !matches!(
                    value,
                    GenericKvLogEvent::UserCreds(_)
                        | GenericKvLogEvent::DeviceCreds(_)
                        | GenericKvLogEvent::LocalRemovalEnvelope(_)
                ) {
                    server_repo.save(value).await?;
                }
            }
            let server = Arc::new(ServerApp::new(server_repo.clone(), seed.master.clone())?);
            server.init().await?;
            Ok::<_, anyhow::Error>((seed, secret, recovery, parallel, server_repo, server))
        })?;
        let http = Http::start(server)?;
        let protocol = Arc::new(HttpSyncProtocol {
            api_url: ApiUrl::custom_dev("http://127.0.0.1", u32::from(http.port)),
        });
        let gateway = Arc::new(SyncGateway::new(
            "actual-native-producer".into(),
            p.clone(),
            protocol.clone(),
            seed.master.clone(),
        ));
        let service = Arc::new(MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer {
                dt: MetaClientRequestTransfer::new(),
            }),
            sync_gateway: gateway.clone(),
            state_provider: Arc::new(MetaClientStateProvider::new()),
            p_obj: p.clone(),
            device_data: seed.approver.device(),
            master_key: seed.master.clone(),
        });
        let app_manager = ApplicationManager::new(protocol, gateway, service.clone(), seed.master);
        let manager = Arc::new(MobileApplicationManager { app_manager });
        let previous = GLOBAL_APP_MANAGER.lock().unwrap().replace(manager);
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                tokio::select! {_ = Box::pin(service.run())=>{},_ = stopped=>{}}
            });
        });
        Ok(Self {
            _dir: dir,
            p,
            actor: seed.approver,
            initiator: seed.initiator,
            target: seed.target,
            secret,
            recovery,
            parallel,
            server_repo,
            http,
            previous,
            stop: Some(stop),
            thread: Some(thread),
        })
    }
    pub fn canonical_delta(&self, kind: CanonicalDelta) -> Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            if matches!(kind, CanonicalDelta::VaultRevision) {
                let mut vault = PersistentVault::from(self.p.clone())
                    .get_vault(self.actor.vault_name.clone())
                    .await?;
                vault.0.key = vault.0.key.next();
                self.server_repo.save(vault.clone()).await?;
                self.p.repo.save(vault).await?;
            } else {
                let server = PersistentSharedSecret::from(Arc::new(PersistentObject::new(
                    self.server_repo.clone(),
                )));
                let mut log = server.get_ss_log_obj(self.actor.vault_name.clone()).await?;
                let other = self.parallel.as_ref().context("PARALLEL_REQUIRED")?;
                log.claims
                    .insert(other.claim.id.clone(), other.claim.clone());
                log.liveboard_recovery_results
                    .insert(other.claim.id.clone(), other.result.clone());
                self.server_repo.save(other.packet.clone()).await?;
                self.p.repo.save(other.packet.clone()).await?;
                let next = server
                    .create_new_ss_log_object(log, self.actor.vault_name.clone())
                    .await?;
                self.server_repo.save(next.clone()).await?;
                self.p.repo.save(next).await?;
            }
            Ok(())
        })
    }
    pub fn completion(&self, claim: &SsClaim) -> Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| anyhow::anyhow!("RUNTIME"))?;
        let ss =
            PersistentSharedSecret::from(Arc::new(PersistentObject::new(self.server_repo.clone())));
        let log = runtime
            .block_on(ss.get_ss_log_obj(self.actor.vault_name.clone()))
            .map_err(|_| anyhow::anyhow!("SERVER_COMPLETION"))?;
        let terminal = log.claims.get(&claim.id).context("SERVER_CLAIM")?;
        ensure!(
            terminal.status.statuses.get(self.actor.device_id())
                == Some(&SsDistributionStatus::Delivered),
            "SERVER_DELIVERED"
        );
        let receiver = terminal
            .approved_recovery_receiver()
            .map_err(|_| anyhow::anyhow!("SERVER_RECEIVER"))?;
        ensure!(
            Some(receiver) == claim.approved_recovery_receiver().ok(),
            "SAME_APPROVED_RECEIVER"
        );
        Ok(())
    }
    pub fn compare_context(a: &Value, context: &Value, changed: Option<&str>) -> Result<()> {
        for field in [
            "version",
            "vaultName",
            "currentDeviceId",
            "vaultSource",
            "localDistributionSource",
            "recoverySources",
            "claimScopeCommitment",
        ] {
            ensure!(
                !a[field].is_null() && !context[field].is_null(),
                "CONTEXT_PRESENT_{field}"
            );
            ensure!(
                (a[field] != context[field]) == (changed == Some(field)),
                "CONTEXT_EQUALITY_{field}"
            );
        }
        ensure!((a != context) == changed.is_some(), "FULL_ORIGINAL_CONTEXT");
        Ok(())
    }
    pub fn sources(&self) -> Result<Vec<DeviceLogObject>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(
            self.p
                .get_object_events_from_beginning(DeviceLogDescriptor::from(self.actor.user_id())),
        )
    }
    pub fn healthy_state(&self) -> Result<()> {
        let result: serde_json::Value = serde_json::from_str(&crate::json_api::get_state())?;
        anyhow::ensure!(
            result["success"] == true,
            "actual native global manager must return real Member state"
        );
        result["message"]["state"]["vault"]["member"]
            .as_object()
            .context("real initialized native Member")?;
        Ok(())
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        *GLOBAL_APP_MANAGER.lock().unwrap() = self.previous.take();
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            assert!(thread.join().is_ok(), "owned native Core service must stop");
        }
        // Then HTTP owner and isolated temp database close; no production path is used.
    }
}

// Opt-in only: the existing one-Pass constructors retain their original setup.
async fn seed_other(h: &mut seed_fixture::Harness) -> Result<MetaPasswordId> {
    let pass = PlainPassInfo::new("native-other-producer".into(), OTHER_VALUE.into());
    let id = pass.pass_id.clone();
    h.vault = h.vault.clone().add_secret(id.clone());
    h.save_vault().await?;
    let member = VaultMember {
        member: UserDataMember::from(h.initiator.user()),
        vault: h.vault.clone(),
    };
    MetaDistributor {
        p_obj: h.p.clone(),
        user_creds: Arc::new(h.initiator.clone()),
        vault_member: member.clone(),
    }
    .distribute(member, SecurePassInfo::from(pass))
    .await?;
    let ss = PersistentSharedSecret::from(h.p.clone());
    let claim = ss
        .find_ss_device_log_tail_event(h.initiator.device_id().clone())
        .await?
        .context("SPLIT_REQUIRED")?
        .to_distribution_request();
    ss.save_ss_log_event(claim).await?;
    Ok(id)
}

async fn seed_parallel(
    h: &seed_fixture::Harness,
    first: &MetaPasswordId,
    second: MetaPasswordId,
    delayed: bool,
) -> Result<(SsClaim, Parallel)> {
    // Prepare actual encrypted approval material on a private copy, not a receipt.
    // Only legal Pending/Accepted snapshots below enter the owned native history.
    let staged = seed_fixture::Harness {
        p: Arc::new(PersistentObject::new(Arc::new(
            super::fault_repo::Repo::default(),
        ))),
        initiator: h.initiator.clone(),
        approver: h.approver.clone(),
        target: h.target.clone(),
        master: h.master.clone(),
        vault: h.vault.clone(),
        protocol: h.protocol.clone(),
    };
    for event in h.p.repo.get_db().await.into_values() {
        staged.p.repo.save(event).await?;
    }
    let local = PersistentSharedSecret::from(h.p.clone());
    let source = PersistentSharedSecret::from(staged.p.clone());
    let mut claims = Vec::new();
    let mut other = None;
    for (pass, is_other) in [(first, false), (&second, true)] {
        let mut claim = super::recovered_show::seed_accepted(&staged, pass).await?;
        claim
            .status
            .statuses
            .insert(h.target.device_id().clone(), SsDistributionStatus::Pending);
        let mut packets = source.get_recoveries(claim.clone()).await?;
        anyhow::ensure!(packets.len() == 1, "ONE_APPROVED_PACKET");
        let packet = packets.remove(0);
        let result = source
            .get_ss_log_obj(h.vault.vault_name.clone())
            .await?
            .liveboard_recovery_results
            .remove(&claim.id)
            .context("ACCEPTED_RESULT_REQUIRED")?;
        let mut initial = claim.clone();
        if is_other && delayed {
            initial
                .status
                .statuses
                .values_mut()
                .for_each(|status| *status = SsDistributionStatus::Pending);
        } else {
            h.p.repo.save(packet.clone()).await?;
        }
        local.save_ss_log_event(initial).await?;
        if !(is_other && delayed) {
            let mut log = local.get_ss_log_obj(h.vault.vault_name.clone()).await?;
            log.liveboard_recovery_results
                .insert(claim.id.clone(), result.clone());
            h.p.repo
                .save(
                    local
                        .create_new_ss_log_object(log, h.vault.vault_name.clone())
                        .await?,
                )
                .await?;
        }
        if is_other {
            other = Some(Parallel {
                pass: second.clone(),
                claim: claim.clone(),
                packet,
                result,
            });
        }
        claims.push(claim);
    }
    Ok((claims.remove(0), other.context("PARALLEL_REQUIRED")?))
}
