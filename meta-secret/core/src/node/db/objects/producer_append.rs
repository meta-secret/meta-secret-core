//! Actual reserved source and readback survive a failed append response.
use super::{
    local_removal_envelope::PersistentRemovalEnvelope, persistent_device_log::PersistentDeviceLog,
};
use crate::node::{
    common::model::{
        liveboard_producer::{
            ProducerAttemptStage, ProducerSourceObservation, ProducerSourceRef,
            producer_source_commitment,
        },
        user::common::UserData,
    },
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor, vault_descriptor::DeviceLogDescriptor,
        },
        events::{
            generic_log_event::ToGenericEvent,
            kv_log_event::{KvKey, KvLogEvent},
            local_removal_envelope::{LocalRemovalEnvelopeEntry, RemovalEnvelopeMode},
            local_remove_producer::LocalRemoveProducerContext,
            object_id::ArtifactId,
            vault::{
                device_log_event::DeviceLogObject,
                vault_log_event::{VaultActionEvent, VaultActionUpdateEvent},
            },
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};

pub(crate) struct ProducerAppend {
    pub event: DeviceLogObject,
    pub stage: ProducerAttemptStage,
    pub persisted: bool,
    pub context: Option<LocalRemoveProducerContext>,
}

pub(crate) enum LocalProducerAppend {
    Request {
        admission_vault: ArtifactId,
        target: crate::node::common::model::device::common::DeviceId,
    },
    Decision(Box<LocalRemoveProducerContext>),
}

impl<R: KvLogEventRepo> PersistentDeviceLog<R> {
    pub(crate) async fn ensure_producer_slot(
        &self,
        user: &UserData,
        event: &DeviceLogObject,
    ) -> Result<()> {
        let registry = PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        };
        if let Some(context) = registry.producer_context(&user.user_id()).await? {
            for source in std::iter::once(&context.request_stage.source)
                .chain(context.decision_stage.as_ref().map(|d| &d.source))
            {
                if source.event_id == event.0.key.obj_id {
                    ensure!(
                        source.body_commitment == producer_source_commitment(event)?,
                        "OperationUnavailable"
                    );
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn save_reserved_device_event(
        &self,
        event: DeviceLogObject,
    ) -> Result<ArtifactId> {
        let id = event.0.key.obj_id.clone();
        let saved = self.p_obj.repo.save(event.clone()).await;
        let read = self
            .p_obj
            .repo
            .find_one(id.clone())
            .await
            .map_err(|_| super::persistent_object::AppendOutcomeUnknown)?;
        ensure!(
            read == Some(event.to_generic()),
            "DeviceLog append outcome unavailable or mismatched"
        );
        if let Ok(key) = saved {
            ensure!(key == id, "DeviceLog append returned another key");
        }
        Ok(id)
    }

    pub(crate) async fn append_producer(
        &self,
        user: &UserData,
        action: VaultActionEvent,
        local: Option<LocalProducerAppend>,
    ) -> Result<ProducerAppend> {
        let _owner = self.p_obj.device_log_append_guard.lock().await;
        let descriptor = DeviceLogDescriptor::from(user.user_id());
        let id = self.free_transport_id(&user.user_id()).await?;
        let event = DeviceLogObject(KvLogEvent {
            key: KvKey::artifact(descriptor.to_obj_desc(), id),
            value: action,
        });
        self.attempt_producer(user, event, local).await
    }

    pub(crate) async fn resume_producer(
        &self,
        user: &UserData,
        event: DeviceLogObject,
        context: LocalRemoveProducerContext,
    ) -> Result<ProducerAppend> {
        let _owner = self.p_obj.device_log_append_guard.lock().await;
        let expected = context
            .decision_stage
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OperationUnavailable"))?;
        ensure!(
            expected.source.event_id == event.0.key.obj_id
                && expected.source.body_commitment == producer_source_commitment(&event)?,
            "OperationUnavailable"
        );
        if let Some(stored) = self
            .p_obj
            .repo
            .find_one(expected.source.event_id.clone())
            .await?
        {
            ensure!(stored == event.clone().to_generic(), "OperationUnavailable");
            return Ok(ProducerAppend {
                event,
                stage: ProducerAttemptStage {
                    source: expected.source.clone(),
                    observation: ProducerSourceObservation::Persisted,
                },
                persisted: true,
                context: Some(context),
            });
        }
        ensure!(
            self.free_transport_id(&user.user_id()).await? == expected.source.event_id,
            "OperationUnavailable"
        );
        self.attempt_producer(
            user,
            event,
            Some(LocalProducerAppend::Decision(Box::new(context))),
        )
        .await
    }

    async fn attempt_producer(
        &self,
        user: &UserData,
        event: DeviceLogObject,
        local: Option<LocalProducerAppend>,
    ) -> Result<ProducerAppend> {
        let id = event.0.key.obj_id.clone();
        let mut stage = ProducerAttemptStage {
            source: ProducerSourceRef {
                event_id: id.clone(),
                body_commitment: producer_source_commitment(&event)?,
            },
            observation: ProducerSourceObservation::Unconfirmed,
        };
        let registry = PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        };
        let mut context = local.map(|intent| match intent {
            LocalProducerAppend::Request { admission_vault, target } => LocalRemoveProducerContext {
                owner: user.user_id(), admission_vault, target, request_stage: stage.clone(), decision_stage: None,
                request_id: None, material_stage: crate::node::common::model::liveboard_producer::ProducerMaterialStage::NotStarted,
            },
            LocalProducerAppend::Decision(mut c) => { c.decision_stage = Some(stage.clone()); *c },
        });
        if let Some(c) = &context
            && registry.save_producer_context(c).await.is_err()
        {
            return Ok(ProducerAppend {
                event,
                stage,
                persisted: false,
                context,
            });
        }
        if matches!(
            event.0.value,
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(_))
        ) {
            let registered = async {
                let owned = registry.load(&user.user_id()).await?.is_some_and(|r| {
                    r.0.value
                        .entries
                        .iter()
                        .any(|e| e.event_id == id && e.mode == RemovalEnvelopeMode::Legacy)
                });
                if !owned {
                    registry
                        .register(
                            &user.user_id(),
                            LocalRemovalEnvelopeEntry {
                                event_id: id.clone(),
                                mode: RemovalEnvelopeMode::Legacy,
                                request_id: None,
                            },
                        )
                        .await?;
                }
                Ok::<(), anyhow::Error>(())
            }
            .await;
            if registered.is_err() {
                return Ok(ProducerAppend {
                    event,
                    stage,
                    persisted: false,
                    context,
                });
            }
        }
        let persisted = self.save_reserved_device_event(event.clone()).await.is_ok();
        if persisted {
            stage.observation = ProducerSourceObservation::Persisted;
        }
        let mut durable = persisted;
        if let Some(c) = &mut context {
            if c.decision_stage.is_some() {
                c.decision_stage = Some(stage.clone());
            } else {
                c.request_stage = stage.clone();
            }
            durable &= registry.save_producer_context(c).await.is_ok();
        }
        Ok(ProducerAppend {
            event,
            stage,
            persisted: durable,
            context,
        })
    }
}
