//! Explicit existing local confirmation resumes only the exact scoped R/U.
use super::{meta_client_service::MetaClientService, producer_receipt};
use crate::node::{
    app::{orchestrator::MetaOrchestrator, sync::sync_protocol::SyncProtocol},
    common::model::{liveboard_producer::*, user::user_creds::UserCreds},
    db::{
        actions::sign_up::remove::{RemoveAction, RemoveActionUpdate},
        descriptors::{
            object_descriptor::ToObjectDescriptor, vault_descriptor::DeviceLogDescriptor,
        },
        events::{
            kv_log_event::{KvKey, KvLogEvent},
            local_remove_producer::LocalRemoveProducerContext,
            vault::{
                device_log_event::DeviceLogObject,
                vault_log_event::{
                    RemoveMemberRequestEvent, VaultActionEvent, VaultActionRequestEvent,
                    VaultActionUpdateEvent,
                },
            },
        },
        objects::{
            local_removal_envelope::PersistentRemovalEnvelope,
            persistent_device_log::PersistentDeviceLog, persistent_vault::PersistentVault,
            producer_append::LocalProducerAppend,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, bail, ensure};

impl<R: KvLogEventRepo, S: SyncProtocol> MetaClientService<R, S> {
    #[tracing::instrument(skip_all)]
    pub async fn continue_local_remove_checked(
        &self,
        reference: ProducerReference,
    ) -> Result<LiveboardProducerReceipt> {
        let _owner = self.p_obj.client_operation_guard.lock().await;
        let creds = self.find_user_creds().await?;
        let mut reference = ProducerReference::parse_json(&serde_json::to_string(&reference)?)?;
        ensure!(
            reference.kind == ProducerKind::RemoveDevice,
            "OperationUnavailable"
        );
        let context = PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        }
        .producer_context(&creds.user().user_id())
        .await?
        .ok_or_else(|| anyhow::anyhow!("OperationUnavailable"))?;
        ensure!(
            context.request_stage.source == reference.request_stage
                && reference
                    .request_id
                    .as_ref()
                    .is_none_or(|id| context.request_id.as_ref().is_none_or(|known| known == id))
                && reference.decision_stage.as_ref().is_none_or(|s| context
                    .decision_stage
                    .as_ref()
                    .is_some_and(|d| d.source == *s)),
            "OperationUnavailable"
        );
        reference.decision_stage = context.decision_stage.as_ref().map(|s| s.source.clone());
        if reference.request_id.is_none() {
            reference.request_id = context.request_id.clone();
        }
        let source = self
            .validate_producer_reference(&creds, &reference)
            .await?
            .ok_or_else(|| anyhow::anyhow!("OperationUnavailable"))?;
        let VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request)) =
            source.0.value.clone()
        else {
            bail!("OperationUnavailable");
        };
        ensure!(
            request.target.user_data.device.device_id == context.target,
            "OperationUnavailable"
        );
        self.sync_gateway.confirm_producer_support().await?;
        let mut receipt =
            producer_receipt::attempted(&source, context.request_stage.clone(), Some(&context))?;
        if receipt.request_id.is_none() {
            receipt.request_id = reference.request_id;
        }
        Ok(self
            .finish_local_producer(&creds, request, context, receipt.clone())
            .await
            .unwrap_or(receipt))
    }

    pub(super) async fn finish_local_producer(
        &self,
        creds: &UserCreds,
        request: RemoveMemberRequestEvent,
        mut context: LocalRemoveProducerContext,
        mut receipt: LiveboardProducerReceipt,
    ) -> Result<LiveboardProducerReceipt> {
        let registry = PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        };
        let vault = PersistentVault::from(self.p_obj.clone())
            .get_vault(creds.vault_name.clone())
            .await?;
        let sender = vault
            .0
            .value
            .members()
            .into_iter()
            .find(|m| m.user_data == creds.user())
            .ok_or_else(|| anyhow::anyhow!("OperationUnavailable"))?;
        if vault.0.value.members().len() == 1 {
            return self
                .complete_applied_local_producer(creds, context, receipt)
                .await;
        }
        ensure!(
            vault.0.key.obj_id == context.admission_vault && vault.0.value.members().len() == 2,
            "OperationUnavailable"
        );
        let member = vault.0.value.clone().to_vault_member(sender)?;
        let update = RemoveAction {
            p_obj: self.p_obj.clone(),
            member,
        }
        .build_update(request, RemoveActionUpdate::Accept, vec![])?;
        let action = VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update.clone()));
        let log = PersistentDeviceLog::from(self.p_obj.clone());
        let append = if let Some(stage) = &context.decision_stage {
            let key = KvKey::artifact(
                DeviceLogDescriptor::from(creds.user().user_id()).to_obj_desc(),
                stage.source.event_id.clone(),
            );
            log.resume_producer(
                &creds.user(),
                DeviceLogObject(KvLogEvent { key, value: action }),
                context,
            )
            .await?
        } else {
            log.append_producer(
                &creds.user(),
                action,
                Some(LocalProducerAppend::Decision(Box::new(context))),
            )
            .await?
        };
        context = append
            .context
            .ok_or_else(|| anyhow::anyhow!("OperationUnavailable"))?;
        producer_receipt::apply_context(&mut receipt, &context);
        if !append.persisted {
            return Ok(receipt);
        }
        context.material_stage = ProducerMaterialStage::Pending;
        if registry.save_producer_context(&context).await.is_err() {
            return Ok(receipt);
        }
        let post_vault = vault.0.value.update_membership(update.new_status);
        let result = MetaOrchestrator {
            p_obj: self.p_obj.clone(),
            user_creds: creds.clone(),
        }
        .finalize_local_remove_material(&post_vault)
        .await;
        context.material_stage = if result.is_ok() {
            ProducerMaterialStage::Ready
        } else {
            ProducerMaterialStage::Unknown
        };
        let saved = registry.save_producer_context(&context).await;
        producer_receipt::apply_context(&mut receipt, &context);
        if result.is_err() || saved.is_err() || self.sync_gateway.sync(creds.user()).await.is_err()
        {
            return Ok(receipt);
        }
        let mut receipt = self.observe_producer_receipt(creds, receipt).await?;
        if receipt.phase == ProducerPhase::Queued
            && receipt.canonical_state == ProducerCanonicalState::Applied
            && ["ready", "done", "settled"].contains(&receipt.readiness.as_str())
        {
            context.request_id = receipt.request_id.clone();
            context.request_stage = receipt.request_stage.clone();
            if let Some(ProducerDecisionStage::Attempted {
                source,
                observation,
            }) = &receipt.decision_stage
            {
                context.decision_stage = Some(ProducerAttemptStage {
                    source: source.clone(),
                    observation: observation.clone(),
                });
            }
            if registry.save_producer_context(&context).await.is_err()
                || registry.clear_producer_context(&context).await.is_err()
            {
                receipt.phase = ProducerPhase::Unknown;
                receipt.readiness = "synchronizing".into();
                receipt.error_code = Some("OperationUnavailable".into());
            }
        }
        Ok(receipt)
    }
}
