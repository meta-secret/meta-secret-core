//! Real stored/signed R is withheld while ordinary guarded removal runs first.
use super::{recovery_intake_fixture::Publisher, *};
use meta_secret_core::node::db::events::vault::vault_status::VaultStatusObject;
use meta_secret_core::{
    crypto::key_pair::{KeyPair, TransportDsaKeyPair},
    node::{
        api::{CanonicalDecisionInput, ServerTailRequest},
        app::{
            orchestrator::MetaOrchestrator,
            sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol},
        },
        common::model::{
            meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo},
            secret::{SecretDistributionData, SsDistributionId},
            vault::vault::VaultMember,
        },
        db::{
            actions::recover::RecoveryAction,
            descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
            events::{
                generic_log_event::GenericKvLogEvent,
                shared_secret_event::{SsDeviceLogObject, SsLogObject},
            },
            objects::persistent_shared_secret::PersistentSharedSecret,
            repo::{generic_db::FindOneQuery, persistent_credentials::PersistentCredentials},
        },
    },
    secret::{MetaDistributor, shared_secret::UserShareDto},
};
use std::sync::atomic::Ordering;

#[path = "recovery_inflight_setup_wire.rs"]
mod setup_wire;
use setup_wire::Wire as SetupWire;

pub(super) const VALUE: &str = "inflight-real-two-of-three-material";
pub(super) struct Wire<R: KvLogEventRepo>(pub Arc<ServerApp<R>>);
impl<R: KvLogEventRepo> SyncProtocol for Wire<R> {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        self.0.handle_client_request(request).await
    }
}
pub(super) struct Inflight<R: KvLogEventRepo> {
    pub base: Harness,
    pub local: Arc<PersistentObject<InMemKvLogEventRepo>>,
    pub repo: Arc<R>,
    pub server: Arc<ServerApp<R>>,
    pub publisher: Arc<Publisher>,
    pub pass: MetaPasswordId,
    pub source: SsDeviceLogObject,
    pub signed: SignedAction,
    pub pre_vault: VaultObject,
    pub admission: VaultObject,
    pub ss_base: SsLogObject,
    pub removal_id: String,
}
impl<R: KvLogEventRepo> Inflight<R> {
    pub async fn new(repo: Arc<R>) -> Result<Self> {
        let base = Harness::new().await?;
        let local = Arc::new(PersistentObject::new(Arc::new(
            InMemKvLogEventRepo::default(),
        )));
        for event in base.p_obj.repo.get_db().await.into_values() {
            local.repo.save(event).await?;
        }
        let pass = PlainPassInfo::new("inflight-original".into(), VALUE.into());
        let pass_id = pass.pass_id.clone();
        let mut vault = PersistentVault::from(local.clone())
            .get_vault(base.a.vault_name.clone())
            .await?;
        vault.0.value = vault.0.value.add_secret(pass_id.clone());
        local.repo.replace(vault.clone()).await?; // Starting fixture only, before tested transition.
        for user in [&base.a, &base.b, &base.c] {
            local
                .repo
                .save(VaultStatusObject::new(
                    vault.0.value.status(user.user()),
                    ArtifactId::from(VaultStatusDescriptor::from(user.user_id())),
                ))
                .await?;
        }
        let member = VaultMember {
            member: member(&base.b),
            vault: vault.0.value.clone(),
        };
        MetaDistributor {
            p_obj: local.clone(),
            user_creds: Arc::new(base.b.clone()),
            vault_member: member.clone(),
        }
        .distribute(member, SecurePassInfo::from(pass))
        .await?;
        let shared = PersistentSharedSecret::from(local.clone());
        let split = shared
            .find_ss_device_log_tail_event(base.b.device_id().clone())
            .await?
            .context("real initial Split")?;
        shared.save_ss_log_event(split.0.value.clone()).await?;
        let a = local_share(&local, &pass_id, &base.a).await?;
        let b = local_share(&local, &pass_id, &base.b).await?;
        assert_eq!(a.share_blocks[0].config.threshold, 2);
        assert!(meta_secret_core::recover_from_shares(vec![a.clone()]).is_err());
        assert_eq!(
            meta_secret_core::recover_from_shares(vec![a, b])?.text,
            VALUE
        );
        // Canonical/material fixture is established before any tested transition.
        for event in local.repo.get_db().await.into_values() {
            if matches!(
                event,
                GenericKvLogEvent::Vault(_)
                    | GenericKvLogEvent::VaultLog(_)
                    | GenericKvLogEvent::VaultStatus(_)
                    | GenericKvLogEvent::SsLog(_)
                    | GenericKvLogEvent::SsDeviceLog(_)
            ) {
                repo.save(event).await?;
            }
        }
        let publisher = Arc::new(Publisher::default());
        let server = Arc::new(ServerApp::new_with_invalidation_publisher(
            repo.clone(),
            base.master_key.clone(),
            publisher.clone(),
        )?);
        server.init().await?;
        let receipt = RecoveryAction::from(local.clone())
            .recovery_request_checked(base.a.clone(), pass_id.clone())
            .await?;
        let source = shared
            .find_ss_device_log_tail_event(base.a.device_id().clone())
            .await?
            .context("actual Core-stored initial R")?;
        assert_eq!(receipt.claim_id, Some(source.0.value.id.wasm_id_str()));
        let signed = sign_event(
            source.clone().to_generic(),
            base.a.device_id().clone(),
            &base.a.device_creds.key_manager()?.dsa,
        )?;
        signed.verify(&base.a.device().keys.dsa_pk)?;
        assert!(repo.find_one(source.obj_id()).await?.is_none());
        let ss_base = PersistentSharedSecret::from(Arc::new(PersistentObject::new(repo.clone())))
            .find_ss_log_tail_event(base.a.vault_name.clone())
            .await?
            .context("initial canonical Split")?;
        let mut h = Self {
            base,
            local,
            repo,
            server,
            publisher,
            pass: pass_id,
            source,
            signed,
            pre_vault: vault.clone(),
            admission: vault,
            ss_base,
            removal_id: String::new(),
        };
        h.remove_c().await?;
        Ok(h)
    }
    async fn remove_c(&mut self) -> Result<()> {
        let original = setup_wire::original_material(self).await?;
        let request = MetaOrchestrator {
            p_obj: self.local.clone(),
            user_creds: self.base.b.clone(),
        }
        .request_remove_member(member(&self.base.c))
        .await?;
        assert_eq!(request.share_transfers.len(), 1);
        assert_eq!(
            request.share_transfers[0].receiver,
            *self.base.a.device_id()
        );
        let event: DeviceLogObject = self
            .local
            .find_tail_event(DeviceLogDescriptor::from(self.base.b.user_id()))
            .await?
            .context("Core B removal request")?;
        send(
            &self.server,
            sign_event(
                event.to_generic(),
                self.base.b.device_id().clone(),
                &self.base.b.device_creds.key_manager()?.dsa,
            )?,
        )
        .await?;
        self.pull_canonical().await?;
        let log = PersistentVault::from(self.local.clone())
            .get_vault_log_artifact(self.base.a.vault_name.clone())
            .await?;
        let pending = &log.0.value.liveboard_removal_pending;
        assert_eq!(pending.len(), 1);
        self.removal_id = pending[0].request_id.clone();
        let master = TransportDsaKeyPair::generate().sk();
        PersistentCredentials {
            p_obj: self.local.clone(),
            master_key: master.clone(),
        }
        .save_user_creds(self.base.a.clone())
        .await?;
        let wire = Arc::new(SetupWire::new(
            self.server.clone(),
            self.source.clone(),
            self.signed.clone(),
        ));
        let gateway = SyncGateway::new(
            "real-inflight-guarded".into(),
            self.local.clone(),
            wire.clone(),
            master,
        );
        let receipt = gateway
            .decide_remove_member(CanonicalDecisionInput {
                request_id: self.removal_id.clone(),
                accepted: true,
            })
            .await?;
        let body = self
            .local
            .repo
            .find_one(receipt.event_id.clone())
            .await?
            .context("durable actual A guarded body")?
            .device_log()?;
        let guarded = gateway
            .sign_queued_device_event(body, &self.base.a.user())
            .await?;
        assert!(matches!(
            guarded.payload,
            meta_secret_core::node::api::SignedActionPayload::GuardedRemovalDecision(_)
        ));
        setup_wire::assert_prepared_unchanged(self, &original).await?;
        wire.expect_guarded(guarded.clone())?;
        // Real guarded admission/import/publication precedes the exact older SS R.
        let sync = gateway.sync(self.base.a.user()).await;
        wire.assert_boundary(sync, &guarded)?;
        setup_wire::assert_applied(self, &guarded).await?;
        for remaining in [&self.base.a, &self.base.b] {
            let share = local_share(&self.local, &self.pass, remaining).await?;
            assert_eq!(
                share.share_blocks[0].config.threshold, 1,
                "actual imported ownApplied published post-removal full replicas"
            );
            assert_eq!(
                meta_secret_core::recover_from_shares(vec![share])?.text,
                VALUE
            );
        }
        assert_eq!(
            local_share(&self.local, &self.pass, &self.base.c)
                .await?
                .share_blocks[0]
                .config
                .threshold,
            2,
            "removed C received no new full copy"
        );
        setup_wire::assert_removed_material_unchanged(self, &original).await?;
        self.assert_withheld().await?;
        assert!(
            self.repo.find_one(receipt.event_id).await?.is_some(),
            "actual DeviceLog approval is consumed while older SS R is still withheld"
        );
        self.admission = PersistentVault::from(Arc::new(PersistentObject::new(self.repo.clone())))
            .get_vault(self.base.a.vault_name.clone())
            .await?;
        assert_eq!(self.admission.0.value.members().len(), 2);
        assert!(self.admission.0.value.is_member(self.base.a.device_id()));
        assert!(self.admission.0.value.is_member(self.base.b.device_id()));
        assert!(matches!(
            self.admission.0.value.find_user(self.base.c.device_id()),
            Some(UserMembership::Outsider(_))
        ));
        self.admission.0.value.ensure_secret_available(&self.pass)?;
        let log = PersistentVault::from(Arc::new(PersistentObject::new(self.repo.clone())))
            .get_vault_log_artifact(self.base.a.vault_name.clone())
            .await?;
        let result = log
            .0
            .value
            .liveboard_removal_results
            .iter()
            .find(|r| r.request_id == self.removal_id)
            .context("actual Applied removal")?;
        assert_eq!(serde_json::to_value(result)?["applicationState"], "applied");
        assert_ne!(self.admission.obj_id(), self.pre_vault.obj_id());
        self.ss_base = self.log().await?;
        self.publisher.0.store(0, Ordering::SeqCst);
        Ok(())
    }
    pub async fn pull_canonical(&self) -> Result<()> {
        // Actual authoritative records only; no synthesized membership or pending binding.
        let p = Arc::new(PersistentObject::new(self.repo.clone()));
        let name = self.base.a.vault_name.clone();
        let vaults = p
            .get_object_events_from_beginning(VaultDescriptor::from(name.clone()))
            .await?;
        let logs = p
            .get_object_events_from_beginning(VaultLogDescriptor::from(name))
            .await?;
        let records = vaults
            .into_iter()
            .map(|record| record.to_generic())
            .chain(logs.into_iter().map(|record| record.to_generic()));
        for event in records {
            if self.local.repo.find_one(event.obj_id()).await?.is_some() {
                self.local.repo.replace(event).await?;
            } else {
                self.local.repo.save(event).await?;
            }
        }
        Ok(())
    }
    pub async fn assert_withheld(&self) -> Result<()> {
        assert!(self.repo.find_one(self.source.obj_id()).await?.is_none());
        assert!(Arc::new(PersistentObject::new(self.repo.clone())).find_tail_id_by_obj_desc(meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsDeviceLogDescriptor::from(self.base.a.device_id().clone())).await?.is_none(), "server SS tail has not consumed original A R or a later source");
        assert!(
            !self
                .log()
                .await?
                .0
                .value
                .claims
                .contains_key(&self.source.0.value.id)
        );
        assert!(
            self.local.repo.find_one(self.source.obj_id()).await?
                == Some(self.source.clone().to_generic())
        );
        Ok(())
    }
    pub async fn log(&self) -> Result<SsLogObject> {
        PersistentSharedSecret::from(Arc::new(PersistentObject::new(self.repo.clone())))
            .find_ss_log_tail_event(self.base.a.vault_name.clone())
            .await?
            .context("canonical SS head")
    }
    pub fn tail(&self) -> SyncRequest {
        SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(ServerTailRequest {
            producer_reference: None,
            sender: self.base.a.user(),
        })))
    }
}
pub(super) async fn local_share(
    p: &Arc<PersistentObject<InMemKvLogEventRepo>>,
    pass: &MetaPasswordId,
    user: &UserCreds,
) -> Result<UserShareDto> {
    let distribution: SecretDistributionData = p
        .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: pass.clone(),
            receiver: user.device_id().clone(),
        }))
        .await?
        .context("real encrypted own share")?
        .to_distribution_data()?;
    let plain = distribution
        .secret_message
        .cipher_text()
        .decrypt(&user.device_creds.secret_box.transport.sk)?;
    Ok(UserShareDto::try_from(&plain.msg)?)
}
