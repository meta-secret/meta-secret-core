//! Updates only the separate compact local context in the existing registry.
use super::local_removal_envelope::PersistentRemovalEnvelope;
use crate::node::{
    common::model::{
        liveboard_producer::{ProducerKind, ProducerMaterialStage, ProducerReference},
        user::common::UserId,
    },
    db::{
        descriptors::{
            local_removal_envelope::LocalRemovalEnvelopeDescriptor,
            object_descriptor::ToObjectDescriptor, vault_descriptor::DeviceLogDescriptor,
        },
        events::{
            generic_log_event::ObjIdExtractor,
            kv_log_event::{KvKey, KvLogEvent},
            local_removal_envelope::{LocalRemovalEnvelopeObject, LocalRemovalEnvelopeRegistry},
            local_remove_producer::LocalRemoveProducerContext,
            object_id::{ArtifactId, Next},
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};

impl<R: KvLogEventRepo> PersistentRemovalEnvelope<R> {
    pub(crate) async fn clear_producer_context(
        &self,
        context: &LocalRemoveProducerContext,
    ) -> Result<()> {
        let Some(mut object) = self.load(&context.owner).await? else {
            anyhow::bail!("OperationUnavailable");
        };
        ensure!(
            object.0.value.producer_context.as_ref() == Some(context),
            "OperationUnavailable"
        );
        object.0.value.producer_context = None;
        object.0.key.obj_id = object.obj_id().next();
        self.p_obj.save_verified(object).await?;
        ensure!(
            self.producer_context(&context.owner).await?.is_none(),
            "OperationUnavailable"
        );
        Ok(())
    }

    pub(crate) async fn producer_context(
        &self,
        user: &UserId,
    ) -> Result<Option<LocalRemoveProducerContext>> {
        let context = self
            .load(user)
            .await?
            .and_then(|r| r.0.value.producer_context);
        if let Some(c) = &context {
            ensure!(
                c.owner == *user && c.target != user.device_id,
                "OperationUnavailable"
            );
            if let Some(id) = &c.request_id {
                ensure!(
                    crate::node::api::is_liveboard_request_id(id),
                    "OperationUnavailable"
                );
            }
            let reference = ProducerReference {
                version: 1,
                kind: ProducerKind::RemoveDevice,
                request_id: c.request_id.clone(),
                request_stage: c.request_stage.source.clone(),
                decision_stage: c.decision_stage.as_ref().map(|d| d.source.clone()),
            };
            ProducerReference::parse_json(&serde_json::to_string(&reference)?)?;
            let own = ArtifactId::from(DeviceLogDescriptor::from(user.clone())).fqdn;
            ensure!(
                reference.request_stage.event_id.fqdn == own
                    && reference
                        .decision_stage
                        .as_ref()
                        .is_none_or(|s| s.event_id.fqdn == own)
                    && (c.material_stage == ProducerMaterialStage::NotStarted
                        || c.decision_stage.is_some()),
                "OperationUnavailable"
            );
        }
        Ok(context)
    }

    pub(crate) async fn save_producer_context(
        &self,
        context: &LocalRemoveProducerContext,
    ) -> Result<()> {
        let previous = self.load(&context.owner).await?;
        if let Some(old) = previous
            .as_ref()
            .and_then(|r| r.0.value.producer_context.as_ref())
        {
            ensure!(
                old.owner == context.owner
                    && old.target == context.target
                    && old.admission_vault == context.admission_vault
                    && old.request_stage.source == context.request_stage.source
                    && old.decision_stage.as_ref().is_none_or(|d| context
                        .decision_stage
                        .as_ref()
                        .is_some_and(|next| next.source == d.source))
                    && old
                        .request_id
                        .as_ref()
                        .is_none_or(|id| context.request_id.as_ref() == Some(id)),
                "OperationUnavailable"
            );
        }
        let descriptor = LocalRemovalEnvelopeDescriptor(context.owner.clone());
        let key = previous
            .as_ref()
            .map(|r| KvKey::artifact(descriptor.clone().to_obj_desc(), r.obj_id().next()))
            .unwrap_or_else(|| KvKey::from(descriptor));
        let mut registry = previous
            .map(|r| r.0.value)
            .unwrap_or(LocalRemovalEnvelopeRegistry {
                owner: context.owner.clone(),
                first_managed_removal_nonce: context.request_stage.source.event_id.id.curr as u64,
                entries: vec![],
                producer_context: None,
            });
        registry.producer_context = Some(context.clone());
        self.p_obj
            .save_verified(LocalRemovalEnvelopeObject(KvLogEvent {
                key,
                value: registry,
            }))
            .await?;
        ensure!(
            self.producer_context(&context.owner).await?.as_ref() == Some(context),
            "OperationUnavailable"
        );
        Ok(())
    }
}
