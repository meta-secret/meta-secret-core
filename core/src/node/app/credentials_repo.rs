use anyhow::anyhow;
use tracing::instrument;

use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, UnitEvent};
use crate::node::db::events::local::CredentialsObject;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use std::sync::Arc;
use crate::node::common::model::device::DeviceName;
use crate::node::common::model::user::UserCredentials;
use crate::node::common::model::vault::VaultName;

pub struct CredentialsRepo<Repo: KvLogEventRepo> {
    pub repo: Arc<Repo>
}

impl<Repo: KvLogEventRepo> CredentialsRepo<Repo> {

    pub async fn save(&self, creds: CredentialsObject) -> anyhow::Result<ObjectId> {
        let generic_event = GenericKvLogEvent::Credentials(creds);
        self.repo.save(generic_event).await
    }

    #[instrument(skip_all)]
    pub async fn find(&self) -> anyhow::Result<Option<CredentialsObject>> {
        let maybe_creds = self.repo
            .find_one(CredentialsObject::unit_id())
            .await?;

        if let None = maybe_creds {
            return Ok(None);
        }

        if let Some(creds) = maybe_creds {
            if let GenericKvLogEvent::Credentials(creds_obj) = creds {
                return Ok(Some(creds_obj))
            } else {
                Err(anyhow!("Credentials index: Invalid event type: {:?}", creds.key().obj_desc()))
            }
        } else {
            Ok(None)
        }
    }

    #[instrument(skip_all)]
    pub async fn generate_user_creds(&self, device_name: DeviceName, vault_name: VaultName) -> anyhow::Result<UserCredentials> {
        let creds = UserCredentials::generate(device_name, vault_name);
        let creds_obj = CredentialsObject::unit(creds.clone());

        self.save(creds_obj.clone()).await?;

        Ok(creds)
    }

    #[instrument(skip_all)]
    pub async fn get_or_generate_user_creds(&self, device_name: DeviceName, vault_name: VaultName) -> anyhow::Result<UserCredentials> {
        let maybe_creds = self.find().await?;

        match maybe_creds {
            None => {
                self.generate_user_creds(device_name, vault_name).await
            },
            Some(creds) => {
                match creds {
                    CredentialsObject::Device { event } => {
                        let user_creds = UserCredentials::from(event.value, vault_name);
                        self.save(CredentialsObject::unit(user_creds.clone())).await?;
                        Ok(user_creds)
                    }
                    CredentialsObject::User { event } => {
                        Ok(event.value)
                    }
                }
            }
        }
    }
}
