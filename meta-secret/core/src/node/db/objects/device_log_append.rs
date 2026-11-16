//! All local DeviceLog producers share this one reservation/readback boundary.
use super::{
    local_removal_envelope::PersistentRemovalEnvelope, persistent_device_log::PersistentDeviceLog,
};
use crate::node::common::model::user::common::UserData;
use crate::node::db::{
    descriptors::{object_descriptor::ToObjectDescriptor, vault_descriptor::DeviceLogDescriptor},
    events::{
        kv_log_event::{KvKey, KvLogEvent},
        local_removal_envelope::{LocalRemovalEnvelopeEntry, RemovalEnvelopeMode},
        object_id::ArtifactId,
        vault::{
            device_log_event::DeviceLogObject,
            vault_log_event::{VaultActionEvent, VaultActionUpdateEvent},
        },
    },
    repo::generic_db::KvLogEventRepo,
};
use anyhow::{Result, ensure};

impl<R: KvLogEventRepo> PersistentDeviceLog<R> {
    pub(crate) async fn append_owned(
        &self,
        sender: &UserData,
        action: VaultActionEvent,
        guarded: Option<String>,
    ) -> Result<ArtifactId> {
        let _owner = self.p_obj.device_log_append_guard.lock().await;
        let event = self
            .reserve_owned_device_event(sender, action, guarded)
            .await?;
        self.save_reserved_device_event(event).await
    }

    /// Caller already holds the shared append owner; never call append_owned here.
    pub(super) async fn reserve_owned_device_event(
        &self,
        sender: &UserData,
        action: VaultActionEvent,
        guarded: Option<String>,
    ) -> Result<DeviceLogObject> {
        let descriptor = DeviceLogDescriptor::from(sender.user_id());
        let id = self.free_transport_id(&sender.user_id()).await?;
        let event = DeviceLogObject(KvLogEvent {
            key: KvKey::artifact(descriptor.to_obj_desc(), id.clone()),
            value: action.clone(),
        });
        self.ensure_producer_slot(sender, &event).await?;
        let registry = PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        };
        registry.clear_orphan(&sender.user_id(), &id).await?;
        let removal = matches!(
            action,
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(_))
        );
        ensure!(
            guarded.is_none() || removal,
            "guarded ownership requires removal decision"
        );
        if removal {
            registry
                .register(
                    &sender.user_id(),
                    LocalRemovalEnvelopeEntry {
                        event_id: id.clone(),
                        mode: if guarded.is_some() {
                            RemovalEnvelopeMode::Guarded
                        } else {
                            RemovalEnvelopeMode::Legacy
                        },
                        request_id: guarded,
                    },
                )
                .await?;
        }
        Ok(event)
    }
}
