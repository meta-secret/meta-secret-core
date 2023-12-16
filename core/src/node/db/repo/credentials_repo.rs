use std::sync::Arc;

use anyhow::{anyhow, Error};
use tracing::instrument;

use crate::node::common::model::device::{DeviceCredentials, DeviceName};
use crate::node::common::model::user::UserCredentials;
use crate::node::common::model::vault::VaultName;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent, UnitEvent};
use crate::node::db::events::local::CredentialsObject;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct CredentialsRepo<Repo: KvLogEventRepo> {
    pub repo: Arc<Repo>,
}

impl<Repo: KvLogEventRepo> CredentialsRepo<Repo> {
    pub async fn save(&self, creds: CredentialsObject) -> anyhow::Result<ObjectId> {
        let generic_event = creds.to_generic();
        self.repo.save(generic_event).await
    }

    #[instrument(skip_all)]
    pub async fn get(&self) -> anyhow::Result<CredentialsObject> {
        self.find().await?
            .ok_or_else(|| anyhow!("No credentials found"))
    }

    #[instrument(skip_all)]
    pub async fn find(&self) -> anyhow::Result<Option<CredentialsObject>> {
        let maybe_creds = self.repo
            .find_one(CredentialsObject::unit_id())
            .await?;

        let Some(creds) = maybe_creds else {
            return Ok(None);
        };

        let creds_obj = CredentialsObject::try_from(creds)?;
        Ok(Some(creds_obj))
    }

    pub async fn generate_device_creds(&self, device_name: DeviceName) -> anyhow::Result<DeviceCredentials> {
        let device_creds = DeviceCredentials::generate(device_name);
        let creds_obj = CredentialsObject::unit(device_creds.clone());
        self.repo
            .save(GenericKvLogEvent::Credentials(creds_obj.clone()))
            .await?;
        Ok(device_creds)
    }

    #[instrument(skip_all)]
    pub async fn generate_user_creds(&self, device_name: DeviceName, vault_name: VaultName) -> anyhow::Result<UserCredentials> {
        let user_creds = UserCredentials::generate(device_name, vault_name);
        let creds_obj = CredentialsObject::default_user(user_creds.clone());

        self.save(creds_obj).await?;

        Ok(user_creds)
    }

    #[instrument(skip_all)]
    pub async fn get_or_generate_user_creds(&self, device_name: DeviceName, vault_name: VaultName) -> anyhow::Result<UserCredentials> {
        let maybe_creds = self.find().await?;

        let Some(creds) = maybe_creds else {
            return self.generate_user_creds(device_name, vault_name).await;
        };

        match creds {
            CredentialsObject::Device { event } => {
                let user_creds = UserCredentials::from(event.value, vault_name);
                self.save(CredentialsObject::default_user(user_creds.clone())).await?;
                Ok(user_creds)
            }
            CredentialsObject::DefaultUser { event } => {
                Ok(event.value)
            }
        }
    }
}
