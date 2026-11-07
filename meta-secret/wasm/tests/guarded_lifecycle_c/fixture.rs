use super::network::Network;
use anyhow::{Context, Result, ensure};
use meta_secret_core::{
    crypto::keys::TransportSk,
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        api::*,
        app::{orchestrator::MetaOrchestrator, sync::sync_gateway::SyncGateway},
        common::model::{
            meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo},
            secret::SsDistributionId,
            user::{common::*, user_creds::UserCreds},
            vault::{vault::*, vault_data::VaultData},
        },
        db::{
            descriptors::{shared_secret_descriptor::SsWorkflowDescriptor, vault_descriptor::*},
            events::{
                generic_log_event::*,
                kv_log_event::*,
                object_id::ArtifactId,
                vault::{
                    device_log_event::DeviceLogObject, vault_event::VaultObject,
                    vault_log_event::*, vault_status::VaultStatusObject,
                },
            },
            in_mem_db::InMemKvLogEventRepo,
            objects::{
                persistent_object::PersistentObject,
                persistent_shared_secret::PersistentSharedSecret,
                persistent_vault::PersistentVault,
            },
            repo::{generic_db::*, persistent_credentials::PersistentCredentials},
        },
        security::sign_event,
    },
    secret::{MetaDistributor, shared_secret::UserShareDto},
};
use meta_server_node::server::server_app::ServerApp;
use std::{collections::HashMap, sync::Arc};

pub type Snapshot = HashMap<ArtifactId, GenericKvLogEvent>;
pub struct Harness {
    pub p: Arc<PersistentObject<InMemKvLogEventRepo>>,
    pub a: UserCreds,
    pub b: UserCreds,
    pub c: UserCreds,
    pub passes: Vec<MetaPasswordId>,
    pub network: Arc<Network>,
    pub server_repo: Arc<InMemKvLogEventRepo>,
    pub server_master: TransportSk,
    pub request_id: String,
    master: TransportSk,
}
impl Harness {
    pub async fn new() -> Result<Self> {
        let state = FixtureRegistry::empty().state;
        let a = state.user_creds.client;
        let b = state.user_creds.client_b;
        let c = state.user_creds.vd;
        let seed = Arc::new(PersistentObject::new(Arc::new(
            InMemKvLogEventRepo::default(),
        )));
        let mut vault = VaultData::from(UserDataMember::from(b.user()));
        for user in [&a, &c] {
            vault = vault.update_membership(UserMembership::Member(user.user().into()));
        }
        seed.repo
            .save(VaultLogObject::create(b.user().into()))
            .await?;
        for user in [&a, &b, &c] {
            seed.repo
                .save(VaultStatusObject::new(
                    VaultStatus::Member(user.user().into()),
                    ArtifactId::from(VaultStatusDescriptor::from(user.user_id())),
                ))
                .await?;
        }
        let mut passes = vec![];
        for index in 0..2 {
            let pass = PlainPassInfo::new(format!("C1-secret-{index}"), value(index).into());
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
            shared.save_ss_log_event(claim).await?; // Initial canonical fixture, not a signed lifecycle assertion.
        }
        let local = Arc::new(InMemKvLogEventRepo::default());
        let server_repo = Arc::new(InMemKvLogEventRepo::default());
        for event in seed.repo.get_db().await.into_values() {
            local.save(event.clone()).await?;
            server_repo.save(event).await?;
        }
        let server = Arc::new(ServerApp::new(
            server_repo.clone(),
            state.device_creds.server_master_key.clone(),
        )?);
        server.init().await?;
        // Real Core A producer + exact existing signing helper + real ordered server.
        let request = MetaOrchestrator {
            p_obj: seed.clone(),
            user_creds: a.clone(),
        }
        .request_remove_member(c.user().into())
        .await?;
        ensure!(
            request.share_transfers.len() == 2,
            "both real Secret inputs must be transferred"
        );
        let body: DeviceLogObject = seed
            .find_tail_event(DeviceLogDescriptor::from(a.user_id()))
            .await?
            .context("actual A request body")?;
        server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                sign_event(
                    body.to_generic(),
                    a.device_id().clone(),
                    &a.device_creds.key_manager()?.dsa,
                )?,
            ))))
            .await?;
        let p = Arc::new(PersistentObject::new(local.clone()));
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
            if local.find_one(event.obj_id()).await?.is_some() {
                local.replace(event).await?;
            } else {
                local.save(event).await?;
            }
        }
        let canonical = PersistentVault::from(p.clone())
            .get_vault_log_artifact(b.vault_name.clone())
            .await?;
        ensure!(
            canonical.0.value.liveboard_removal_pending.len() == 1,
            "actual server pending UUID required"
        );
        let binding = &canonical.0.value.liveboard_removal_pending[0];
        ensure!(
            binding.request == request,
            "server pending source differs from actual A request"
        );
        let request_id = binding.request_id.clone();
        let master = state.device_creds.client_b_master_key;
        PersistentCredentials {
            p_obj: p.clone(),
            master_key: master.clone(),
        }
        .save_user_creds(b.clone())
        .await?;
        let h = Self {
            p,
            a,
            b,
            c,
            passes,
            network: Arc::new(Network::new(server, local)),
            server_repo,
            server_master: state.device_creds.server_master_key,
            request_id,
            master,
        };
        h.assert_original_k2(&h.p.repo.get_db().await)?;
        Ok(h)
    }
    pub fn gateway(&self) -> SyncGateway<InMemKvLogEventRepo, Network> {
        SyncGateway::new(
            "C1-actual-host".into(),
            self.p.clone(),
            self.network.clone(),
            self.master.clone(),
        )
    }
    pub fn input(&self) -> CanonicalDecisionInput {
        CanonicalDecisionInput {
            request_id: self.request_id.clone(),
            accepted: true,
        }
    }
    pub fn assert_original_k2(&self, snapshot: &Snapshot) -> Result<()> {
        ensure!(self.passes.len() == 2, "two actual Secrets required");
        for (index, pass) in self.passes.iter().enumerate() {
            let mut pair = vec![];
            let mut generation = None;
            for user in [&self.a, &self.b, &self.c] {
                let id = ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
                    pass_id: pass.clone(),
                    receiver: user.device_id().clone(),
                }));
                let data = snapshot
                    .get(&id)
                    .context("actual original encrypted packet missing")?
                    .clone()
                    .shared_secret()?
                    .to_distribution_data()?;
                let plain = data
                    .secret_message
                    .cipher_text()
                    .decrypt(&user.device_creds.secret_box.transport.sk)?;
                let share = UserShareDto::try_from(&plain.msg)?;
                ensure!(
                    !share.share_blocks.is_empty()
                        && share
                            .share_blocks
                            .iter()
                            .all(|block| block.config.threshold == 2
                                && block.config.number_of_shares == 3),
                    "original current material must remain actual K2-of-3"
                );
                if let Some(ref expected) = generation {
                    ensure!(expected == &data.claim_id, "mixed original generation");
                } else {
                    generation = Some(data.claim_id);
                }
                if user.device_id() != self.c.device_id() {
                    pair.push(share);
                }
            }
            ensure!(
                meta_secret_core::recover_from_shares(pair)?.text == value(index),
                "original survivor K2 pair must recover exact fixture value"
            );
        }
        Ok(())
    }
}
fn value(index: usize) -> &'static str {
    if index == 0 {
        "C1_PRIVATE_FIXTURE_FIRST"
    } else {
        "C1_PRIVATE_FIXTURE_SECOND"
    }
}
pub fn ordinary(snapshot: &Snapshot) -> Snapshot {
    snapshot
        .iter()
        .filter(|(_, event)| {
            matches!(
                event,
                GenericKvLogEvent::SsWorkflow(_)
                    | GenericKvLogEvent::SsDeviceLog(_)
                    | GenericKvLogEvent::SsLog(_)
            )
        })
        .map(|(id, event)| (id.clone(), event.clone()))
        .collect()
}
