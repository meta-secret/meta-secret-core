use std::sync::Arc;
use log::info;
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent, UnitEvent};
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::local_event::CredentialsObject;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use tracing::instrument;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::device::device_creds::DeviceCredentials;

pub struct CredentialsRepo<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> CredentialsRepo<Repo> {
    pub async fn get_or_generate_device_creds(&self, device_name: DeviceName) -> anyhow::Result<DeviceCredentials> {
        let maybe_creds = self.find().await?;

        let device_creds = match maybe_creds {
            None => self.generate_device_creds(device_name).await?,
            Some(creds) => match creds {
                CredentialsObject::Device(event) => event.value,
                CredentialsObject::DefaultUser(event) => event.value.device_creds,
            },
        };
        Ok(device_creds)
    }

    #[instrument(skip_all)]
    pub async fn save(&self, creds: CredentialsObject) -> anyhow::Result<ObjectId> {
        let generic_event = creds.to_generic();
        self.p_obj.repo.save(generic_event).await
    }

    #[instrument(skip_all)]
    pub async fn get_user_creds(&self) -> anyhow::Result<Option<UserCredentials>> {
        let maybe_creds_obj = self.find().await?;

        let Some(creds_obj) = maybe_creds_obj else {
            return Ok(None);
        };

        match creds_obj {
            CredentialsObject::Device { .. } => Ok(None),
            CredentialsObject::DefaultUser(event) => Ok(Some(event.value)),
        }
    }

    #[instrument(skip_all)]
    pub async fn find(&self) -> anyhow::Result<Option<CredentialsObject>> {
        let maybe_creds = self.p_obj.find_tail_event(ObjectDescriptor::CredsIndex).await?;

        let Some(creds) = maybe_creds else {
            return Ok(None);
        };

        let creds_obj = CredentialsObject::try_from(creds)?;
        Ok(Some(creds_obj))
    }

    #[instrument(skip(self))]
    pub async fn generate_device_creds(&self, device_name: DeviceName) -> anyhow::Result<DeviceCredentials> {
        let device_creds = DeviceCredentials::generate(device_name);
        let creds_obj = CredentialsObject::unit(device_creds.clone());
        self.p_obj
            .repo
            .save(GenericKvLogEvent::Credentials(creds_obj.clone()))
            .await?;
        Ok(device_creds)
    }

    #[instrument(skip(self))]
    pub async fn generate_user_creds(
        &self,
        device_name: DeviceName,
        vault_name: VaultName,
    ) -> anyhow::Result<UserCredentials> {
        info!("Generate user creds");
        
        let user_creds = UserCredentials::generate(device_name, vault_name);
        let creds_obj = CredentialsObject::default_user(user_creds.clone());

        self.save(creds_obj).await?;

        Ok(user_creds)
    }

    #[instrument(skip_all)]
    pub async fn get_or_generate_user_creds(
        &self,
        device_name: DeviceName,
        vault_name: VaultName,
    ) -> anyhow::Result<UserCredentials> {
        let maybe_creds = self.find().await?;

        match maybe_creds {
            None => {
                self.get_or_generate_device_creds(device_name.clone()).await?;
                self.generate_user_creds(device_name, vault_name).await
            },
            Some(CredentialsObject::Device(KvLogEvent { value: creds, .. })) => {
                let user_creds = UserCredentials::from(creds, vault_name);
                self.save(CredentialsObject::default_user(user_creds.clone())).await?;
                Ok(user_creds)
            }
            Some(CredentialsObject::DefaultUser(event)) => Ok(event.value),
        }
    }
}

#[cfg(test)]
pub mod fixture {
    
}

#[cfg(test)]
pub mod spec {
    use std::sync::Arc;
    use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::repo::generic_db::KvLogEventRepo;

    pub struct CredentialsRepoSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
    }

    impl CredentialsRepoSpec<InMemKvLogEventRepo> {
        pub async fn verify(&self) -> anyhow::Result<()> {
            let creds_desc = ObjectDescriptor::CredsIndex;
            
            let events =  self.p_obj
                .get_object_events_from_beginning(creds_desc)
                .await?;
            
            assert_eq!(events.len(), 2);
            
            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use tracing_attributes::instrument;
    use crate::meta_tests::setup_tracing;
    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::vault::VaultName;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::repo::credentials_repo::CredentialsRepo;
    use crate::node::db::repo::credentials_repo::spec::CredentialsRepoSpec;

    #[tokio::test]
    #[instrument]
    async fn test_get_or_generate_user_creds() -> anyhow::Result<()> {
        setup_tracing()?;

        let p_obj = Arc::new(PersistentObject::in_mem());
        
        let creds_repo = CredentialsRepo {p_obj: p_obj.clone() };
        let vault_name = VaultName::from("test");
        let _ = creds_repo
            .get_or_generate_user_creds(DeviceName::server(), vault_name)
            .await?;

        let spec = CredentialsRepoSpec {p_obj: p_obj.clone()};
        spec.verify().await?;
        
        Ok(())
    }
}
