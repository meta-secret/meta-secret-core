use std::sync::Arc;
use log::info;
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::generic_log_event::{ToGenericEvent, UnitEvent};
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::local_event::CredentialsObject;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use tracing::instrument;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::device::device_creds::DeviceCredentials;

pub struct PersistentCredentials<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentCredentials<Repo> {

    #[instrument(skip(self))]
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

    #[instrument(skip(self))]
    async fn save(&self, creds: CredentialsObject) -> anyhow::Result<ObjectId> {
        let generic_event = creds.to_generic();
        self.p_obj.repo.save(generic_event).await
    }

    #[instrument(skip(self))]
    pub async fn save_device_creds(&self, device_creds: &DeviceCredentials) -> anyhow::Result<()> {
        let creds_obj = CredentialsObject::unit(device_creds.clone());
        let generic_event = creds_obj.to_generic();
        self.p_obj.repo.save(generic_event).await?;
        Ok(())
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
    async fn generate_device_creds(&self, device_name: DeviceName) -> anyhow::Result<DeviceCredentials> {
        let device_creds = DeviceCredentials::generate(device_name);
        info!("Device credentials has been generated: {:?}", &device_creds.device);

        self.save_device_creds(&device_creds).await?;
        Ok(device_creds)
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
                let device_creds = self.get_or_generate_device_creds(device_name.clone()).await?;
                let user_creds = UserCredentials::from(device_creds, vault_name);
                self.save(CredentialsObject::default_user(user_creds.clone())).await?;
                Ok(user_creds)
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
    use std::sync::Arc;
    use crate::meta_tests::fixture_util::fixture::states::EmptyState;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::repo::persistent_credentials::PersistentCredentials;

    pub struct PersistentCredentialsFixture {
        pub client_p_creds: Arc<PersistentCredentials<InMemKvLogEventRepo>>,
        pub vd_p_creds: Arc<PersistentCredentials<InMemKvLogEventRepo>>,
        pub server_p_creds: Arc<PersistentCredentials<InMemKvLogEventRepo>>
    }

    impl PersistentCredentialsFixture {
        pub async fn init(state: &EmptyState) -> anyhow::Result<Self> {
            let client_p_creds = Arc::new(PersistentCredentials {
                p_obj: state.p_obj.client.clone(),
            });

            let vd_p_creds = Arc::new(PersistentCredentials {
                p_obj: state.p_obj.vd.clone(),
            });

            let server_p_creds = Arc::new(PersistentCredentials {
                p_obj: state.p_obj.server.clone(),
            });

            let _ = client_p_creds.save_device_creds(&state.device_creds.client).await?;
            let _ = client_p_creds.get_or_generate_user_creds(
                state.device_creds.client.device.device_name.clone(),
                state.user_creds.client.user().vault_name()
            ).await?;

            let _ = vd_p_creds.save_device_creds(&state.device_creds.vd).await?;
            let _ = vd_p_creds.get_or_generate_user_creds(
                state.device_creds.vd.device.device_name.clone(),
                state.user_creds.vd.user().vault_name()
            ).await?;

            let _ = server_p_creds.save_device_creds(&state.device_creds.server).await?;

            Ok(Self { client_p_creds, vd_p_creds, server_p_creds })
        }
    }

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
    use crate::node::db::repo::persistent_credentials::PersistentCredentials;
    use crate::node::db::repo::persistent_credentials::spec::CredentialsRepoSpec;

    #[tokio::test]
    #[instrument]
    async fn test_get_or_generate_user_creds() -> anyhow::Result<()> {
        setup_tracing()?;

        let p_obj = Arc::new(PersistentObject::in_mem());
        
        let creds_repo = PersistentCredentials {p_obj: p_obj.clone() };
        let vault_name = VaultName::from("test");
        let _ = creds_repo
            .get_or_generate_user_creds(DeviceName::server(), vault_name)
            .await?;

        let spec = CredentialsRepoSpec {p_obj: p_obj.clone()};
        spec.verify().await?;
        
        Ok(())
    }
}
