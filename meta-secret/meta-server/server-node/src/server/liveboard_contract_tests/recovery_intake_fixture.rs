//! B2 fixtures construct the exact original Core-created, genuinely signed source.
use super::super::state_invalidation::{StateInvalidation, StateInvalidationPublisher};
use super::*;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        api::{ServerTailRequest, SsRequest},
        common::model::{secret::SsLogData, vault::vault::VaultMember},
        db::{
            descriptors::shared_secret_descriptor::{SsDeviceLogDescriptor, SsLogDescriptor},
            events::{
                shared_secret_event::{SsDeviceLogObject, SsLogObject},
                vault::recovery_request_intake::RecoveryRequestIntake,
            },
            objects::persistent_shared_secret::PersistentSharedSecret,
        },
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
pub(super) struct Publisher(pub AtomicUsize);
impl StateInvalidationPublisher for Publisher {
    fn publish(&self, _: StateInvalidation) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
pub(super) struct IntakeHarness<R: KvLogEventRepo> {
    pub base: Harness,
    pub repo: Arc<R>,
    pub source: SsDeviceLogObject,
    pub signed: SignedAction,
    pub base_log: SsLogObject,
    pub prepared: SsLogObject,
    pub publisher: Arc<Publisher>,
}
impl<R: KvLogEventRepo> IntakeHarness<R> {
    pub async fn new(repo: Arc<R>) -> Result<Self> {
        let base = Harness::new().await?;
        for event in base.p_obj.repo.get_db().await.into_values() {
            repo.save(event).await?;
        }
        let p = Arc::new(PersistentObject::new(repo.clone()));
        let mut vault = PersistentVault::from(p.clone())
            .get_vault(base.a.vault_name.clone())
            .await?;
        let pass = meta_secret_core::node::common::model::meta_pass::MetaPasswordId::build_from_str(
            "initial-intake",
        );
        vault.0.value = vault.0.value.add_secret(pass.clone());
        repo.replace(vault.clone()).await?;
        let vm = VaultMember {
            member: member(&base.a),
            vault: vault.0.value.clone(),
        };
        let claim = vm.create_recovery_claim(pass);
        let source = SsDeviceLogObject(KvLogEvent {
            key: KvKey::from(SsDeviceLogDescriptor::from(base.a.device_id().clone())),
            value: claim.clone(),
        });
        let signed = sign_event(
            source.clone().to_generic(),
            base.a.device_id().clone(),
            &base.a.device_creds.key_manager()?.dsa,
        )?;
        signed.verify(&base.a.device().keys.dsa_pk)?;
        let base_log = SsLogObject(KvLogEvent {
            key: KvKey::from(SsLogDescriptor::from(base.a.vault_name.clone())),
            value: SsLogData::default(),
        });
        repo.save(base_log.clone()).await?;
        let target = base_log.obj_id().next();
        let receipt = RecoveryRequestIntake {
            version: 1,
            claim_id: claim.id.clone(),
            requester_id: claim.sender.clone(),
            source_key: source.0.key.clone(),
            source_body_hash: sha256_hex(&String::from_utf8(canonical_json(&source)?)?),
            verified_action_hash: sha256_hex(&String::from_utf8(canonical_json(&signed)?)?),
            verified_vault_artifact_id: vault.obj_id(),
            base_ss_log_artifact_id: Some(base_log.obj_id()),
            target_ss_log_artifact_id: target.clone(),
        };
        let mut prepared = base_log.clone();
        prepared.0.key.obj_id = target;
        prepared.0.value = prepared.0.value.insert(claim);
        prepared.0.value.pending_recovery_request_intake = Some(receipt);
        Ok(Self {
            base,
            repo,
            source,
            signed,
            base_log,
            prepared,
            publisher: Arc::new(Publisher::default()),
        })
    }
    pub async fn server(&self) -> Result<ServerApp<R>> {
        let server = ServerApp::new_with_invalidation_publisher(
            self.repo.clone(),
            self.base.master_key.clone(),
            self.publisher.clone(),
        )?;
        server.init().await?;
        self.publisher.0.store(0, Ordering::SeqCst);
        Ok(server)
    }
    pub fn tail(&self) -> SyncRequest {
        SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(ServerTailRequest {
            producer_reference: None,
            sender: self.base.a.user(),
        })))
    }
    pub fn ss(&self, id: ArtifactId) -> SyncRequest {
        SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(SsRequest {
            sender: self.base.a.user(),
            ss_log: id,
        })))
    }
    pub fn vault(&self, slot: usize, id: ArtifactId) -> SyncRequest {
        let mut tail = VaultTail {
            vault_status: ArtifactId::from(VaultStatusDescriptor::from(self.base.a.user_id())),
            vault_log: ArtifactId::from(VaultLogDescriptor::from(self.base.a.vault_name.clone())),
            vault: ArtifactId::from(VaultDescriptor::from(self.base.a.vault_name.clone())),
        };
        match slot {
            0 => tail.vault_status = id,
            1 => tail.vault_log = id,
            _ => tail.vault = id,
        }
        SyncRequest::Read(Box::new(ReadSyncRequest::Vault(VaultRequest {
            sender: self.base.a.user(),
            tail,
        })))
    }
    pub async fn log(&self) -> Result<SsLogObject> {
        PersistentSharedSecret::from(Arc::new(PersistentObject::new(self.repo.clone())))
            .find_ss_log_tail_event(self.base.a.vault_name.clone())
            .await?
            .context("canonical SsLog")
    }
    pub async fn seed_prepared(&self) -> Result<()> {
        self.repo.save(self.prepared.clone()).await?;
        Ok(())
    }
    pub async fn assert_settled(&self) -> Result<()> {
        let log = self.log().await?;
        assert_eq!(
            log.obj_id(),
            self.prepared.obj_id(),
            "cleanup must replace the same fixed target"
        );
        assert!(log.0.value.pending_recovery_request_intake.is_none());
        assert!(log.0.value.claims.get(&self.source.0.value.id) == Some(&self.source.0.value));
        assert!(
            self.repo.find_one(self.source.obj_id()).await?
                == Some(self.source.clone().to_generic())
        );
        assert!(log.0.value.liveboard_recovery_results.is_empty());
        Ok(())
    }
}

pub(super) fn assert_clean(response: &DataSyncResponse) -> Result<()> {
    assert!(
        !serde_json::to_string(response)?.contains("pendingRecoveryRequestIntake"),
        "Prepared metadata was serialized by an actual Data response"
    );
    Ok(())
}
