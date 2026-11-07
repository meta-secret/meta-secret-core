//! Current-member own-next-slot writes. No historical/business decision authority.
use super::server_data_sync::ServerSyncGateway;
use anyhow::{Result, anyhow, ensure};
use meta_secret_core::node::{
    common::model::{device::common::DeviceId, user::common::UserMembership},
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::{
            device_transport_noop::DeviceTransportNoopObject, generic_log_event::*,
            object_id::ArtifactId,
        },
        objects::{persistent_device_log::PersistentDeviceLog, persistent_vault::PersistentVault},
        repo::generic_db::KvLogEventRepo,
    },
    device_transport_noop::{DeviceTransportNoop, DeviceTransportNoopReceipt, next_transport_id},
};

impl<R: KvLogEventRepo> ServerSyncGateway<R> {
    pub(super) async fn apply_transport_noop(
        &self,
        authorization: DeviceTransportNoop,
    ) -> Result<DeviceTransportNoopReceipt> {
        authorization.validate_shape()?;
        let vault = available(
            PersistentVault::from(self.p_obj.clone())
                .get_vault(authorization.vault_name.clone())
                .await,
        )?
        .to_data();
        let Some(UserMembership::Member(member)) = vault.find_user(&authorization.actor_id) else {
            anyhow::bail!("DeviceTransportNoopUnauthorized");
        };
        let user = &member.user_data;
        ensure!(
            user.vault_name == authorization.vault_name
                && user.device.device_id == authorization.actor_id
                && DeviceId::from(&user.device.keys) == authorization.actor_id,
            "DeviceTransportNoopUnauthorized"
        );
        // This verifier has no event-actor/bootstrap-key fallback.
        let intended = DeviceTransportNoopObject::new(
            authorization,
            &user.user_id(),
            &user.device.keys.dsa_pk,
        )?;
        let id = intended.obj_id();
        if let Some(existing) = available(self.p_obj.repo.find_one(id.clone()).await)? {
            ensure!(
                existing == intended.clone().to_generic(),
                "DeviceTransportNoopConflict"
            );
            return Ok(intended.receipt());
        }
        let log = PersistentDeviceLog::from(self.p_obj.clone());
        let next = match available(log.find_tail_id(&user.user_id()).await)? {
            Some(tail) => next_transport_id(&tail)?,
            None => ArtifactId::from(DeviceLogDescriptor::from(user.user_id())),
        };
        ensure!(id == next, "DeviceTransportNoopNotNext");
        let saved = self.p_obj.repo.save(intended.clone()).await;
        ensure!(
            available(self.p_obj.repo.find_one(id.clone()).await)?
                == Some(intended.clone().to_generic()),
            "DeviceTransportNoopUnavailable"
        );
        if let Ok(saved) = saved {
            ensure!(saved == id, "DeviceTransportNoopUnavailable");
        }
        Ok(intended.receipt())
    }
}

fn available<T>(result: Result<T>) -> Result<T> {
    result.map_err(|_| anyhow!("DeviceTransportNoopUnavailable"))
}
