//! Real signed Recovery packets and repositories; no successful crypto/storage mocks.
use super::*;
use meta_secret_core::crypto::{key_pair::KeyPair, utils::sha256_hex};
use meta_secret_core::node::api::{SsRecoveryCompletion, SsRequest};
use meta_secret_core::node::common::model::{
    crypto::aead::EncryptedMessage,
    secret::{SsClaim, SsDistributionStatus, SsLogData},
};
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsLogDescriptor;
use meta_secret_core::node::db::events::shared_secret_event::SsWorkflowObject;
use meta_secret_core::node::db::events::vault::liveboard_metadata::DecisionOutcome;
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use meta_secret_core::node::db::repo::recovery_response_store::{
    PrivateRecoveryResponse, RecoveryResponsePhase, RecoveryResponseSourceProof,
};
use meta_secret_core::node::security::sign_recovery_completion;
use meta_secret_core::secret::shared_secret::PlainText;

pub(super) struct ReceiptHarness<R: KvLogEventRepo> {
    pub base: Harness,
    pub repo: Arc<R>,
    pub claim: SsClaim,
}

impl<R: KvLogEventRepo> ReceiptHarness<R> {
    pub async fn new(repo: Arc<R>) -> Result<Self> {
        let base = Harness::new().await?;
        let claim = super::recovery::pending_claim(&base).await?;
        // Copy only actual canonical fixture events into the selected real backend.
        for event in base.p_obj.repo.get_db().await.into_values() {
            repo.save(event).await?;
        }
        Ok(Self { base, repo, claim })
    }
    pub async fn server(&self) -> Result<ServerApp<R>> {
        let server = ServerApp::new(self.repo.clone(), self.base.master_key.clone())?;
        server.init().await?;
        Ok(server)
    }
    pub async fn log(&self) -> Result<SsLogData> {
        PersistentSharedSecret::from(Arc::new(PersistentObject::new(self.repo.clone())))
            .get_ss_log_obj(self.base.a.vault_name.clone())
            .await
    }
    pub fn packet(&self, actor: &UserCreds, accepted: bool) -> Result<SignedAction> {
        let id = self
            .claim
            .recovery_db_ids()
            .into_iter()
            .find(|id| &id.distribution_id.receiver == actor.device_id())
            .context("addressed receiver")?;
        if !accepted {
            let workflow = SsWorkflowObject::Decline(KvLogEvent {
                key: KvKey::from(meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor::Decline(id)),
                value: meta_secret_core::node::common::model::secret::SsDeclineData {
                    vault_name: self.claim.vault_name.clone(), claim_id: self.claim.id.clone(), receiver_id: actor.device_id().clone(),
                },
            });
            return sign_event(
                workflow.to_generic(),
                actor.device_id().clone(),
                &actor.device_creds.key_manager()?.dsa,
            );
        }
        let share = actor.device_creds.key_manager()?.transport.encrypt_string(
            PlainText::from("private-receipt-synthetic-share"),
            &self.base.a.device_creds.key_manager()?.transport.pk(),
        )?;
        let workflow = SsWorkflowObject::Recovery(KvLogEvent {
            key: KvKey::from(meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor::Recovery(id)),
            value: meta_secret_core::node::common::model::secret::SecretDistributionData {
                vault_name: self.claim.vault_name.clone(), claim_id: self.claim.dist_claim_id.clone(),
                secret_message: EncryptedMessage::CipherShare { share },
            },
        });
        sign_event(
            workflow.to_generic(),
            actor.device_id().clone(),
            &actor.device_creds.key_manager()?.dsa,
        )
    }
    pub async fn prepared(
        &self,
        packet: SignedAction,
        accepted: bool,
    ) -> Result<PrivateRecoveryResponse> {
        let p_ss = PersistentSharedSecret::from(Arc::new(PersistentObject::new(self.repo.clone())));
        let base = p_ss
            .find_ss_log_tail_event(self.claim.vault_name.clone())
            .await?
            .context("canonical claim log")?
            .obj_id();
        Ok(PrivateRecoveryResponse {
            version: 1,
            vault_name: self.claim.vault_name.clone(),
            claim_id: self.claim.id.clone(),
            base_ss_log_artifact_id: base.clone(),
            target_ss_log_artifact_id: base.next(),
            source_proof: RecoveryResponseSourceProof {
                workflow_artifact_id: packet.event()?.obj_id(),
                verified_envelope_hash: sha256_hex(&String::from_utf8(canonical_json(&packet)?)?),
                responder_id: packet.signer.clone(),
                outcome: if accepted {
                    DecisionOutcome::Accepted
                } else {
                    DecisionOutcome::Declined
                },
            },
            phase: RecoveryResponsePhase::Prepared,
            packet: Some(packet),
        })
    }
    pub async fn assert_winner(&self, accepted: bool) -> Result<Value> {
        let log = self.log().await?;
        let record = log
            .liveboard_recovery_results
            .get(&self.claim.id)
            .context("RED: first response winner missing")?;
        assert!(
            record.responder_id == *self.base.b.device_id(),
            "reserved responder must remain the winner"
        );
        assert!(
            record.outcome
                == if accepted {
                    DecisionOutcome::Accepted
                } else {
                    DecisionOutcome::Declined
                }
        );
        let status = &log.claims[&self.claim.id].status;
        assert!(
            status.get(self.base.b.device_id())
                == Some(&if accepted {
                    SsDistributionStatus::Sent
                } else {
                    SsDistributionStatus::Declined
                })
        );
        Ok(serde_json::to_value(record)?)
    }
    pub fn ss_request(&self, actor: &UserCreds, id: Option<ArtifactId>) -> SyncRequest {
        SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(SsRequest {
            sender: actor.user(),
            ss_log: id.unwrap_or_else(|| {
                ArtifactId::from(SsLogDescriptor::from(self.claim.vault_name.clone()))
            }),
        })))
    }
    pub fn completion(&self) -> Result<SyncRequest> {
        let recovery_id = self
            .claim
            .recovery_db_ids()
            .into_iter()
            .find(|id| &id.distribution_id.receiver == self.base.b.device_id())
            .context("approved receiver")?;
        let action = sign_recovery_completion(
            SsRecoveryCompletion {
                vault_name: self.claim.vault_name.clone(),
                recovery_id,
                receiver_status: SsDistributionStatus::Sent,
            },
            self.base.a.device_id().clone(),
            &self.base.a.device_creds.key_manager()?.dsa,
        )?;
        Ok(SyncRequest::Read(Box::new(
            ReadSyncRequest::SsRecoveryCompletion(action),
        )))
    }
}
