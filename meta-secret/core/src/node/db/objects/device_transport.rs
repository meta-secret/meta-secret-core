//! Explicit transport union. Generic object traversal never skips record kinds.
use super::{persistent_device_log::PersistentDeviceLog, persistent_vault::PersistentVault};
use crate::node::{
    common::model::user::common::{UserData, UserId},
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor, vault_descriptor::DeviceLogDescriptor,
        },
        events::{
            device_transport_noop::DeviceTransportNoopObject,
            generic_log_event::*,
            object_id::ArtifactId,
            vault::{device_log_event::DeviceLogObject, vault_log_event::*},
        },
        repo::generic_db::KvLogEventRepo,
    },
    device_transport_noop::{next_transport_id, validate_transport_id},
};
use anyhow::{Result, bail, ensure};

#[derive(Clone, Debug)]
pub enum DeviceTransportSlot {
    Business(Box<DeviceLogObject>),
    Noop(Box<DeviceTransportNoopObject>),
}
impl DeviceTransportSlot {
    pub fn obj_id(&self) -> ArtifactId {
        match self {
            Self::Business(e) => e.obj_id(),
            Self::Noop(e) => e.obj_id(),
        }
    }
    pub fn business(self) -> Option<DeviceLogObject> {
        match self {
            Self::Business(e) => Some(*e),
            Self::Noop(_) => None,
        }
    }
}

pub fn device_event_actor(event: &VaultActionEvent) -> &UserData {
    match event {
        VaultActionEvent::Init(VaultActionInitEvent::CreateVault(e)) => &e.owner.user_data,
        VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(e)) => &e.candidate,
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(e)) => &e.sender.user_data,
        VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(e)) => &e.sender.user_data,
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(e)) => &e.sender.user_data,
        VaultActionEvent::Update(VaultActionUpdateEvent::UpdateMembership(e)) => {
            &e.sender.user_data
        }
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(e)) => &e.sender.user_data,
        VaultActionEvent::Update(VaultActionUpdateEvent::AddMetaPass(e)) => &e.sender.user_data,
        VaultActionEvent::Update(VaultActionUpdateEvent::DeleteSecret(e)) => &e.sender.user_data,
        VaultActionEvent::Update(VaultActionUpdateEvent::AddToPending { candidate }) => candidate,
    }
}

impl<R: KvLogEventRepo> PersistentDeviceLog<R> {
    pub(crate) async fn free_transport_id(&self, user: &UserId) -> Result<ArtifactId> {
        match self.find_tail_id(user).await? {
            Some(tail) => next_transport_id(&tail),
            None => Ok(ArtifactId::from(DeviceLogDescriptor::from(user.clone()))),
        }
    }
    pub async fn transport_slot(
        &self,
        user: &UserId,
        id: ArtifactId,
    ) -> Result<Option<DeviceTransportSlot>> {
        validate_transport_id(&id)?;
        let descriptor = DeviceLogDescriptor::from(user.clone());
        ensure!(
            id.fqdn == ArtifactId::from(descriptor.clone()).fqdn,
            "InvalidDeviceTransportSlot"
        );
        let Some(raw) = self.p_obj.repo.find_one(id.clone()).await? else {
            return Ok(None);
        };
        ensure!(
            raw.key().obj_id == id && raw.key().obj_desc == descriptor.to_obj_desc(),
            "InvalidDeviceTransportSlot"
        );
        match raw {
            GenericKvLogEvent::DeviceLog(event) => {
                let actor = device_event_actor(&event.0.value);
                ensure!(
                    actor.user_id() == *user && event.0.value.vault_name() == user.vault_name,
                    "InvalidDeviceTransportSlot"
                );
                Ok(Some(DeviceTransportSlot::Business(Box::new(event))))
            }
            GenericKvLogEvent::DeviceTransportNoop(event) => {
                let vault = PersistentVault::from(self.p_obj.clone())
                    .get_vault(user.vault_name.clone())
                    .await?
                    .to_data();
                let actor = vault
                    .find_user(&user.device_id)
                    .ok_or_else(|| anyhow::anyhow!("InvalidDeviceTransportSlot"))?;
                let actor = actor.user_data();
                ensure!(actor.user_id() == *user, "InvalidDeviceTransportSlot");
                event.validate(user, &actor.device.keys.dsa_pk)?;
                Ok(Some(DeviceTransportSlot::Noop(Box::new(event))))
            }
            _ => bail!("InvalidDeviceTransportSlot"),
        }
    }
    pub async fn transport_slots(
        &self,
        user: &UserId,
        from: ArtifactId,
    ) -> Result<Vec<DeviceTransportSlot>> {
        let mut result = vec![];
        let mut id = from;
        while let Some(slot) = self.transport_slot(user, id.clone()).await? {
            result.push(slot);
            id = next_transport_id(&id)?;
        }
        Ok(result)
    }
}
