//! Real encrypted Remove source, followed by an actual independently approved Delete.
use super::{
    correlation_faults::{CorrelationRepo, Fault, Step, transfer_count},
    correlation_support::{commitment, observe},
    integration_binding_fixture::Seed,
    recovery_inflight_fixture::local_share,
    recovery_intake_fixture::Publisher,
    *,
};
use meta_secret_core::node::{
    app::orchestrator::MetaOrchestrator,
    common::model::{
        liveboard_producer::*,
        meta_pass::{PlainPassInfo, SecurePassInfo},
        secret::SsDistributionId,
        user::common::UserData,
        vault::vault::VaultMember,
    },
    db::{
        descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
        events::vault::{vault_log_event::*, vault_status::VaultStatusObject},
        repo::generic_db::FindOneQuery,
    },
};
use meta_secret_core::secret::MetaDistributor;
use std::sync::atomic::Ordering;

pub(super) type Repo = CorrelationRepo<InMemKvLogEventRepo>;
pub(super) struct GuardFixture {
    pub seed: Seed<Repo>,
    pub server: Arc<ServerApp<Repo>>,
    pub peer: UserCreds,
    pub third: UserCreds,
    pub source: DeviceLogObject,
    pub signed: SignedAction,
    pub publisher: Arc<Publisher>,
    pub before_ref: Option<Value>,
}

impl GuardFixture {
    pub async fn new(admit_before_delete: bool) -> Result<Self> {
        let repo = Arc::new(Repo::new(Arc::new(InMemKvLogEventRepo::default())));
        let (seed, peer, third) = initial_material(repo).await?;
        let local = Arc::new(PersistentObject::in_mem());
        for record in seed.repo.inner.get_db().await.into_values() {
            local.repo.save(record).await?;
        }
        let vault = PersistentVault::from(seed.p.clone())
            .get_vault(seed.actor.vault_name.clone())
            .await?;
        for user in [&seed.actor, &peer, &third] {
            local
                .repo
                .save(VaultStatusObject::new(
                    vault.0.value.status(user.user()),
                    ArtifactId::from(VaultStatusDescriptor::from(user.user_id())),
                ))
                .await?;
        }
        let a = local_share(&local, &seed.pass, &seed.actor).await?;
        let c = local_share(&local, &seed.pass, &third).await?;
        assert_eq!(a.share_blocks[0].config.threshold, 2);
        assert!(meta_secret_core::recover_from_shares(vec![a.clone()]).is_err());
        assert_eq!(
            meta_secret_core::recover_from_shares(vec![a, c])?.text,
            "SYNTHETIC-PRIVATE-DELETE-SECRET"
        );
        let request = MetaOrchestrator {
            p_obj: local.clone(),
            user_creds: seed.actor.clone(),
        }
        .request_remove_member(member(&peer))
        .await?;
        assert_eq!(request.share_transfers.len(), 1);
        assert_eq!(request.share_transfers[0].pass_id, seed.pass);
        assert_eq!(request.share_transfers[0].receiver, *third.device_id());
        let source: DeviceLogObject = local
            .find_tail_event(DeviceLogDescriptor::from(seed.actor.user_id()))
            .await?
            .context("actual Core-saved Remove source")?;
        let signed = sign_event(
            source.clone().to_generic(),
            seed.actor.device_id().clone(),
            &seed.actor.device_creds.key_manager()?.dsa,
        )?;
        signed.verify(&seed.actor.device().keys.dsa_pk)?;
        let publisher = Arc::new(Publisher::default());
        let server = Arc::new(ServerApp::new_with_invalidation_publisher(
            seed.repo.clone(),
            seed.master.clone(),
            publisher.clone(),
        )?);
        server.init().await?;
        let mut fixture = Self {
            seed,
            server,
            peer,
            third,
            source,
            signed,
            publisher,
            before_ref: None,
        };
        if admit_before_delete {
            fixture
                .seed
                .repo
                .arm(&fixture.source, "", Step::Raw, Fault::Before);
            assert!(send(&fixture.server, fixture.signed.clone()).await.is_err());
            assert_eq!(fixture.seed.repo.hits.load(Ordering::SeqCst), 1);
            fixture.seed.repo.clear();
            assert!(
                fixture
                    .seed
                    .repo
                    .inner
                    .find_one(fixture.source.obj_id())
                    .await?
                    .is_none()
            );
            fixture.before_ref = Some(fixture.retained_ref().await?);
        }
        fixture.delete_secret().await?;
        assert!(
            fixture
                .seed
                .repo
                .inner
                .find_one(fixture.source.obj_id())
                .await?
                .is_none()
        );
        assert!(transfer_count(&serde_json::to_value(&fixture.source)?) > 0);
        if let Some(before) = &fixture.before_ref {
            assert_eq!(&fixture.retained_ref().await?, before);
        }
        fixture.publisher.0.store(0, Ordering::SeqCst);
        Ok(fixture)
    }

    async fn delete_secret(&self) -> Result<()> {
        let vault = PersistentVault::from(self.seed.p.clone())
            .get_vault(self.seed.actor.vault_name.clone())
            .await?;
        let request =
            DeleteSecretRequest::new(member(&self.third), self.seed.pass.clone(), &vault.0.value);
        assert!(request.requires_approval());
        self.send_as(
            &self.third,
            VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request.clone())),
        )
        .await?;
        self.send_as(
            &self.peer,
            VaultActionEvent::Update(VaultActionUpdateEvent::DeleteSecret(DeleteSecretDecision {
                request: request.clone(),
                sender: member(&self.peer),
                accepted: true,
            })),
        )
        .await?;
        let vault = PersistentVault::from(self.seed.p.clone())
            .get_vault(self.seed.actor.vault_name.clone())
            .await?;
        assert!(vault.0.value.deleted_secrets.contains(&self.seed.pass));
        assert!(!vault.0.value.secrets.contains(&self.seed.pass));
        let result = vault
            .0
            .value
            .deletion_results
            .get(&request.request_id)
            .context("actual remote approved deletion")?;
        assert!(result.accepted);
        assert_eq!(result.sender, member(&self.peer));
        for id in &self.seed.workflows {
            assert!(self.seed.repo.inner.find_one(id.clone()).await?.is_none());
        }
        Ok(())
    }

    pub async fn send_as(&self, actor: &UserCreds, body: VaultActionEvent) -> Result<()> {
        let source = next_event(&self.seed.p, actor, body).await?;
        send(
            &self.server,
            sign_event(
                source.to_generic(),
                actor.device_id().clone(),
                &actor.device_creds.key_manager()?.dsa,
            )?,
        )
        .await
    }

    pub async fn retained_ref(&self) -> Result<Value> {
        let log = PersistentVault::from(self.seed.p.clone())
            .get_vault_log_artifact(self.seed.actor.vault_name.clone())
            .await?;
        let record = log
            .0
            .value
            .producer_source(&self.source.obj_id())?
            .context("actual source provenance")?;
        assert_eq!(record.body_commitment, commitment(&self.source)?);
        assert_eq!(record.source_event_id, self.source.obj_id());
        assert_eq!(record.actor_id, *self.seed.actor.device_id());
        assert_eq!(
            record.verified_envelope_digest,
            meta_secret_core::crypto::utils::sha256_hex(&String::from_utf8(canonical_json(
                &self.signed
            )?)?)
        );
        Ok(serde_json::to_value(record)?)
    }

    pub async fn reference(&self) -> Result<ProducerReference> {
        let retained = self.retained_ref().await?;
        Ok(ProducerReference {
            version: 1,
            kind: ProducerKind::RemoveDevice,
            request_id: Some(
                retained["requestId"]
                    .as_str()
                    .context("actual removal UUID")?
                    .into(),
            ),
            request_stage: ProducerSourceRef {
                event_id: self.source.obj_id(),
                body_commitment: commitment(&self.source)?,
            },
            decision_stage: None,
        })
    }

    pub async fn assert_clean(&self) -> Result<()> {
        let raw = self
            .seed
            .repo
            .inner
            .find_one(self.source.obj_id())
            .await?
            .context("same source raw slot")?
            .device_log()?;
        assert_eq!(raw.obj_id(), self.source.obj_id());
        assert_eq!(
            transfer_count(&serde_json::to_value(raw)?),
            0,
            "RED: acknowledged producer raw append restored deleted encrypted transfer"
        );
        self.signed.verify(&self.seed.actor.device().keys.dsa_pk)?;
        if let Some(before) = &self.before_ref {
            assert_eq!(&self.retained_ref().await?, before);
        }
        Ok(())
    }
}

async fn initial_material(repo: Arc<Repo>) -> Result<(Seed<Repo>, UserCreds, UserCreds)> {
    let f = FixtureRegistry::empty().state;
    let actor = f.user_creds.client;
    let peer = f.user_creds.client_b;
    let third = f.user_creds.vd;
    let p = Arc::new(PersistentObject::new(repo.clone()));
    let input = PlainPassInfo::new(
        "actual-deleted-transfer".into(),
        "SYNTHETIC-PRIVATE-DELETE-SECRET".into(),
    );
    let pass = input.pass_id.clone();
    let mut vault = VaultObject::sign_up(actor.vault_name.clone(), member(&actor));
    vault.0.value = vault
        .0
        .value
        .update_membership(UserMembership::Member(member(&peer)))
        .update_membership(UserMembership::Member(member(&third)))
        .add_secret(pass.clone());
    let vm = VaultMember {
        member: member(&actor),
        vault: vault.clone().to_data(),
    };
    repo.save(vault).await?;
    repo.save(VaultLogObject::create(member(&actor))).await?;
    MetaDistributor {
        p_obj: p.clone(),
        user_creds: Arc::new(actor.clone()),
        vault_member: vm.clone(),
    }
    .distribute(vm.clone(), SecurePassInfo::from(input))
    .await?;
    let mut workflows = vec![];
    for member in vm.vault.members() {
        workflows.push(
            p.find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass.clone(),
                receiver: member.user_data.device.device_id,
            }))
            .await?
            .context("real original encrypted distribution")?
            .obj_id(),
        );
    }
    assert_eq!(workflows.len(), 3);
    Ok((
        Seed {
            repo,
            p,
            actor,
            master: f.device_creds.server_master_key,
            pass,
            workflows,
        },
        peer,
        third,
    ))
}

pub(super) async fn require_unavailable_or_clean(
    h: &GuardFixture,
    server: &ServerApp<Repo>,
    sender: UserData,
) -> Result<()> {
    if let Ok(response) = observe(server, sender, h.reference().await?).await
        && let Some(proven) = response.producer_observation
        && proven.canonical_state != ProducerCanonicalState::Unknown
    {
        h.assert_clean().await?;
        assert_eq!(
            proven.request_stage.observation,
            ProducerSourceObservation::Redacted
        );
    }
    Ok(())
}

pub(super) async fn public_reads_before_observation(
    h: &GuardFixture,
    server: &ServerApp<Repo>,
) -> Result<()> {
    let request = VaultRequest {
        sender: h.seed.actor.user(),
        tail: VaultTail {
            vault_status: ArtifactId::from(VaultStatusDescriptor::from(h.seed.actor.user_id())),
            vault_log: h.source.obj_id(),
            vault: ArtifactId::from(VaultDescriptor::from(h.seed.actor.vault_name.clone())),
        },
    };
    if let Ok(response) = server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(request))))
        .await
    {
        let raw = response.to_data()?;
        assert!(
            raw.0
                .iter()
                .any(|event| event.obj_id() == h.source.obj_id()),
            "actual supported generic raw source route must execute"
        );
        assert_eq!(
            transfer_count(&serde_json::to_value(raw)?),
            0,
            "RED: ordinary public Generic raw read returned deleted transfer before cleanup"
        );
    }
    if let Ok(response) = server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
            meta_secret_core::node::api::ServerTailRequest {
                sender: h.seed.actor.user(),
                producer_reference: None,
            },
        ))))
        .await
    {
        let next = response.to_server_tail()?.device_log_tail;
        if next == Some(h.source.obj_id().next()) {
            h.assert_clean().await?;
        }
    }
    Ok(())
}
