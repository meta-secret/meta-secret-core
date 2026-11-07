use super::fixtures::Harness;
use anyhow::{Context, Result, ensure};
use meta_secret_core::{
    node::{
        common::model::{
            ApplicationState, VaultFullInfo,
            crypto::aead::EncryptedMessage,
            meta_pass::MetaPasswordId,
            secret::{SecretDistributionData, SsClaim, SsDistributionStatus},
            user::{common::UserDataMember, user_creds::UserCreds},
        },
        db::{
            descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
            events::{
                generic_log_event::*, kv_log_event::*, shared_secret_event::SsWorkflowObject,
            },
            objects::persistent_shared_secret::PersistentSharedSecret,
            repo::generic_db::*,
        },
    },
    secret::shared_secret::PlainText,
};
use serde_json::Value;

pub struct Recovery {
    pub h: Harness,
    pub pass: MetaPasswordId,
    pub claim: SsClaim,
}
impl Recovery {
    pub async fn new() -> Result<Self> {
        let mut h = Harness::new(3).await?;
        let pass = h.seed_secret("source-controls").await?;
        let mut claim = h
            .vault
            .clone()
            .to_vault_member(UserDataMember::from(h.approver.user()))?
            .create_recovery_claim(pass.clone());
        claim
            .status
            .statuses
            .insert(h.initiator.device_id().clone(), SsDistributionStatus::Sent);
        claim
            .status
            .statuses
            .insert(h.target.device_id().clone(), SsDistributionStatus::Declined);
        let result = Self { h, pass, claim };
        result.save_claim(&result.claim).await?;
        result
            .packet(&result.h.initiator, &result.h.initiator)
            .await?;
        ensure!(
            result.context().await?.is_some(),
            "real source fixture must begin actionable with proof"
        );
        Ok(result)
    }
    pub async fn save_claim(&self, claim: &SsClaim) -> Result<()> {
        PersistentSharedSecret::from(self.h.p.clone())
            .save_ss_log_event(claim.clone())
            .await
    }
    pub async fn packet(
        &self,
        owner: &UserCreds,
        encryptor: &UserCreds,
    ) -> Result<SsWorkflowObject> {
        let share = self.h.share(&self.pass, owner).await?;
        let cipher = encryptor
            .device_creds
            .key_manager()?
            .transport
            .encrypt_string(
                PlainText::from(share.as_json()?),
                &self.h.approver.device().keys.transport_pk,
            )?;
        let id = self
            .claim
            .recovery_db_ids()
            .into_iter()
            .find(|id| id.distribution_id.receiver == *owner.device_id())
            .context("exact source receiver")?;
        let packet = SsWorkflowObject::Recovery(KvLogEvent {
            key: KvKey::from(SsWorkflowDescriptor::Recovery(id)),
            value: SecretDistributionData {
                vault_name: self.h.vault.vault_name.clone(),
                claim_id: self.claim.dist_claim_id.clone(),
                secret_message: EncryptedMessage::CipherShare { share: cipher },
            },
        });
        if self.h.p.repo.find_one(packet.obj_id()).await?.is_some() {
            self.h.p.repo.replace(packet.clone()).await?;
        } else {
            self.h.p.repo.save(packet.clone()).await?;
        }
        Ok(packet)
    }
    pub async fn state(&self) -> Result<Value> {
        let ApplicationState::Vault(VaultFullInfo::Member(member)) =
            self.h.service().get_app_state().await?
        else {
            anyhow::bail!("actual current member required")
        };
        Ok(serde_json::to_value(member)?["liveboardState"].clone())
    }
    pub async fn context(&self) -> Result<Option<Value>> {
        let state = self.state().await?;
        let op = state["operations"]
            .as_array()
            .context("operations")?
            .iter()
            .find(|op| op["requestId"] == self.claim.id.wasm_id_str())
            .context("original operation")?;
        Ok(op.get("revealContext").cloned())
    }
    pub async fn source(&self) -> Result<SsWorkflowObject> {
        PersistentSharedSecret::from(self.h.p.clone())
            .get_recoveries(self.claim.clone())
            .await?
            .into_iter()
            .next()
            .context("source")
    }
    pub async fn local(&self) -> Result<SsWorkflowObject> {
        use meta_secret_core::node::common::model::secret::SsDistributionId;
        self.h
            .p
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: self.pass.clone(),
                receiver: self.h.approver.device_id().clone(),
            }))
            .await?
            .context("actual local ciphertext source")
    }
}
