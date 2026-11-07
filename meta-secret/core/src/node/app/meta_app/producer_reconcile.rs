//! Exact atomic observation; no sync, finalization, append, signing or consent.
use super::{meta_client_service::MetaClientService, producer_receipt};
use crate::node::{
    app::sync::sync_protocol::SyncProtocol,
    common::model::{liveboard_producer::*, user::user_creds::UserCreds},
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::{
            generic_log_event::ObjIdExtractor,
            object_id::ArtifactId,
            vault::{
                device_log_event::DeviceLogObject,
                vault_log_event::{
                    VaultActionEvent, VaultActionRequestEvent, VaultActionUpdateEvent,
                },
            },
        },
        objects::{
            local_removal_envelope::PersistentRemovalEnvelope,
            persistent_shared_secret::PersistentSharedSecret, persistent_vault::PersistentVault,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};

impl<R: KvLogEventRepo, S: SyncProtocol> MetaClientService<R, S> {
    pub async fn reconcile_producer_receipt(
        &self,
        input: ProducerReference,
    ) -> Result<LiveboardProducerReceipt> {
        let _owner = self.p_obj.client_operation_guard.lock().await;
        let creds = self.find_user_creds().await?;
        let input = ProducerReference::parse_json(&serde_json::to_string(&input)?)?;
        let context = PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        }
        .producer_context(&creds.user().user_id())
        .await?;
        let context = context.filter(|c| {
            c.request_stage.source == input.request_stage
                && input.kind == ProducerKind::RemoveDevice
        });
        let mut reference = input.clone();
        if let Some(c) = &context {
            ensure!(
                reference
                    .request_id
                    .as_ref()
                    .zip(c.request_id.as_ref())
                    .is_none_or(|(supplied, known)| supplied == known),
                "OperationUnavailable"
            );
            ensure!(
                reference.decision_stage.as_ref().is_none_or(|source| c
                    .decision_stage
                    .as_ref()
                    .is_some_and(|d| d.source == *source)),
                "OperationUnavailable"
            );
            reference.decision_stage = c.decision_stage.as_ref().map(|d| d.source.clone());
            if reference.request_id.is_none() {
                reference.request_id = c.request_id.clone();
            }
        }
        let source = self.validate_producer_reference(&creds, &reference).await?;
        let mut receipt = if let Some(source) = &source {
            producer_receipt::attempted(
                source,
                ProducerAttemptStage {
                    source: reference.request_stage.clone(),
                    observation: ProducerSourceObservation::Persisted,
                },
                context.as_ref(),
            )?
        } else {
            LiveboardProducerReceipt {
                version: 1,
                kind: reference.kind.clone(),
                phase: ProducerPhase::Unknown,
                secret_id: None,
                target_device_id: None,
                request_id: reference.request_id.clone(),
                request_stage: ProducerAttemptStage {
                    source: reference.request_stage.clone(),
                    observation: ProducerSourceObservation::Unconfirmed,
                },
                decision_stage: None,
                material_stage: None,
                canonical_state: ProducerCanonicalState::Unknown,
                readiness: "synchronizing".into(),
                error_code: Some("OperationUnavailable".into()),
            }
        };
        if receipt.request_id.is_none() {
            receipt.request_id = reference.request_id;
        }
        if receipt.decision_stage.is_none()
            && let Some(source) = reference.decision_stage
        {
            receipt.decision_stage = Some(ProducerDecisionStage::Attempted {
                source,
                observation: ProducerSourceObservation::Unconfirmed,
            });
        }
        if let Some(c) = &context {
            producer_receipt::apply_context(&mut receipt, c);
        }
        self.observe_producer_receipt(&creds, receipt).await
    }

    pub(super) async fn validate_producer_reference(
        &self,
        creds: &UserCreds,
        input: &ProducerReference,
    ) -> Result<Option<DeviceLogObject>> {
        let vault = PersistentVault::from(self.p_obj.clone())
            .get_vault(creds.vault_name.clone())
            .await?;
        ensure!(
            vault.0.value.is_member(creds.device_id()),
            "OperationUnavailable"
        );
        let expected = ArtifactId::from(DeviceLogDescriptor::from(creds.user().user_id())).fqdn;
        for source in std::iter::once(&input.request_stage).chain(input.decision_stage.iter()) {
            ensure!(source.event_id.fqdn == expected, "OperationUnavailable");
        }
        let stored = self
            .p_obj
            .repo
            .find_one(input.request_stage.event_id.clone())
            .await?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let event = stored.device_log()?;
        ensure!(
            event.obj_id() == input.request_stage.event_id
                && producer_source_commitment(&event)? == input.request_stage.body_commitment,
            "OperationUnavailable"
        );
        let actor = match &event.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(r))
                if input.kind == ProducerKind::DeleteSecret
                    && input.request_id.as_ref() == Some(&r.request_id) =>
            {
                &r.sender.user_data
            }
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r))
                if input.kind == ProducerKind::RemoveDevice =>
            {
                &r.sender.user_data
            }
            _ => anyhow::bail!("OperationUnavailable"),
        };
        ensure!(*actor == creds.user(), "OperationUnavailable");
        if let Some(reference) = &input.decision_stage
            && let Some(stored) = self.p_obj.repo.find_one(reference.event_id.clone()).await?
        {
            let u = stored.device_log()?;
            ensure!(
                producer_source_commitment(&u)? == reference.body_commitment,
                "OperationUnavailable"
            );
            ensure!(
                matches!(&u.0.value, VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(v)) if v.sender.user_data == creds.user()),
                "OperationUnavailable"
            );
            ensure!(
                matches!((&event.0.value, &u.0.value),
                (VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r)),
                 VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(v)))
                 if v.request == *r && v.target == r.target && v.decision == crate::node::db::events::vault::vault_log_event::RemoveMemberDecision::Accepted),
                "OperationUnavailable"
            );
        }
        Ok(Some(event))
    }

    pub(super) async fn observe_producer_receipt(
        &self,
        creds: &UserCreds,
        mut receipt: LiveboardProducerReceipt,
    ) -> Result<LiveboardProducerReceipt> {
        let reference = producer_receipt::reference(&receipt);
        if let Ok(Some(observation)) = self
            .sync_gateway
            .observe_producer(creds.user(), reference)
            .await
            && producer_receipt::apply_observation(&mut receipt, observation).is_err()
        {
            receipt.phase = ProducerPhase::Unknown;
            receipt.error_code = Some("OperationUnavailable".into());
            return Ok(receipt);
        }
        let request_acked =
            receipt.request_stage.observation == ProducerSourceObservation::RawAcknowledged;
        let decision_acked = match &receipt.decision_stage {
            None => true,
            Some(ProducerDecisionStage::Attempted { observation, .. }) => {
                *observation == ProducerSourceObservation::RawAcknowledged
            }
            Some(ProducerDecisionStage::NotAttempted) => false,
        };
        let material_ready = receipt
            .material_stage
            .as_ref()
            .is_none_or(|s| *s == ProducerMaterialStage::Ready);
        if request_acked && decision_acked && material_ready {
            receipt.phase = ProducerPhase::Queued;
            receipt.error_code = None;
            receipt.readiness = self
                .producer_readiness(creds, &receipt)
                .await
                .unwrap_or_else(|_| "synchronizing".into());
            if receipt.material_stage.is_some()
                && !["ready", "done", "settled"].contains(&receipt.readiness.as_str())
            {
                receipt.phase = ProducerPhase::Unknown;
                receipt.error_code = Some("OperationUnavailable".into());
            }
        }
        Ok(receipt)
    }

    pub(super) async fn producer_readiness(
        &self,
        creds: &UserCreds,
        receipt: &LiveboardProducerReceipt,
    ) -> Result<String> {
        let p = PersistentVault::from(self.p_obj.clone());
        let vault_source = p.get_vault(creds.vault_name.clone()).await?;
        let vault = vault_source.0.value.clone();
        let member = vault
            .members()
            .into_iter()
            .find(|m| m.user_data == creds.user())
            .ok_or_else(|| anyhow::anyhow!("OperationUnavailable"))?;
        let member = vault.to_vault_member(member)?;
        let events = p
            .get_vault_log_artifact(creds.vault_name.clone())
            .await?
            .0
            .value;
        let claims = PersistentSharedSecret::from(self.p_obj.clone())
            .get_ss_log_obj(creds.vault_name.clone())
            .await?
            .with_client_status(creds.device_id());
        if receipt.material_stage.is_some() {
            let material = super::liveboard_material::Material {
                p_obj: self.p_obj.clone(),
                creds,
                member: &member,
                claims: &claims,
            };
            for pass in &member.vault.secrets {
                ensure!(
                    material.local(pass).await?.is_some(),
                    "OperationUnavailable"
                );
            }
        }
        let state = super::liveboard_projection::project(
            self.p_obj.clone(),
            creds,
            &member,
            &vault_source,
            &events,
            &claims,
        )
        .await?;
        Ok(state
            .operations
            .into_iter()
            .find(|op| receipt.request_id.as_ref() == Some(&op.request_id))
            .map(|op| op.readiness)
            .unwrap_or_else(|| "synchronizing".into()))
    }
}
