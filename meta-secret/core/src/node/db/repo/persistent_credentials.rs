use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::device::device_creds::DeviceCredentials;
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::descriptors::creds::CredentialsDescriptor;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::local_event::CredentialsObject;
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::Result;
use derive_more::From;
use log::info;
use std::sync::Arc;
use tracing::instrument;

#[derive(From)]
pub struct PersistentCredentials<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentCredentials<Repo> {
    #[instrument(skip(self))]
    pub async fn get_or_generate_device_creds(
        &self,
        device_name: DeviceName,
    ) -> Result<DeviceCredentials> {
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
    async fn save(&self, creds: CredentialsObject) -> Result<ArtifactId> {
        let generic_event = creds.to_generic();
        self.p_obj.repo.save(generic_event).await
    }

    #[instrument(skip(self))]
    async fn save_device_creds(&self, device_creds: DeviceCredentials) -> Result<()> {
        let creds_obj = CredentialsObject::from(device_creds);
        let generic_event = creds_obj.to_generic();
        self.p_obj.repo.save(generic_event).await?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn get_user_creds(&self) -> Result<Option<UserCredentials>> {
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
    pub async fn find(&self) -> Result<Option<CredentialsObject>> {
        let maybe_device_creds = self
            .p_obj
            .find_tail_event(CredentialsDescriptor::Device)
            .await?;
        let maybe_user_creds = self
            .p_obj
            .find_tail_event(CredentialsDescriptor::User)
            .await?;

        let creds = maybe_user_creds.or_else(|| maybe_device_creds);

        Ok(creds)
    }

    #[instrument(skip(self))]
    async fn generate_device_creds(&self, device_name: DeviceName) -> Result<DeviceCredentials> {
        let device_creds = DeviceCredentials::generate(device_name);
        info!(
            "Device credentials has been generated: {:?}",
            &device_creds.device
        );

        self.save_device_creds(device_creds.clone()).await?;
        Ok(device_creds)
    }

    #[instrument(skip_all)]
    pub async fn get_or_generate_user_creds(
        &self,
        device_name: DeviceName,
        vault_name: VaultName,
    ) -> Result<UserCredentials> {
        let maybe_creds = self.find().await?;

        match maybe_creds {
            None => {
                let device_creds = self
                    .get_or_generate_device_creds(device_name.clone())
                    .await?;
                let user_creds = UserCredentials::from(device_creds, vault_name);
                self.save(CredentialsObject::from(user_creds.clone()))
                    .await?;
                Ok(user_creds)
            }
            Some(CredentialsObject::Device(KvLogEvent { value: creds, .. })) => {
                let user_creds = UserCredentials::from(creds, vault_name);
                self.save(CredentialsObject::from(user_creds.clone()))
                    .await?;
                Ok(user_creds)
            }
            Some(CredentialsObject::DefaultUser(event)) => Ok(event.value),
        }
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::EmptyState;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::repo::persistent_credentials::PersistentCredentials;
    use std::sync::Arc;

    pub struct PersistentCredentialsFixture {
        pub client_p_creds: Arc<PersistentCredentials<InMemKvLogEventRepo>>,
        pub vd_p_creds: Arc<PersistentCredentials<InMemKvLogEventRepo>>,
        pub server_p_creds: Arc<PersistentCredentials<InMemKvLogEventRepo>>,
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

            client_p_creds
                .save_device_creds(state.device_creds.client.clone())
                .await?;

            client_p_creds
                .get_or_generate_user_creds(
                    state.device_creds.client.device.device_name.clone(),
                    state.user_creds.client.user().vault_name(),
                )
                .await?;

            vd_p_creds
                .save_device_creds(state.device_creds.vd.clone())
                .await?;
            vd_p_creds
                .get_or_generate_user_creds(
                    state.device_creds.vd.device.device_name.clone(),
                    state.user_creds.vd.user().vault_name(),
                )
                .await?;

            server_p_creds
                .save_device_creds(state.device_creds.server.clone())
                .await?;

            Ok(Self {
                client_p_creds,
                vd_p_creds,
                server_p_creds,
            })
        }
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod spec {
    use crate::node::db::descriptors::creds::CredentialsDescriptor;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use derive_more::From;
    use std::sync::Arc;

    #[derive(From)]
    pub struct PersistentCredentialsSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
    }

    impl PersistentCredentialsSpec<InMemKvLogEventRepo> {
        pub async fn verify_user_creds(&self) -> anyhow::Result<()> {
            let device_creds = self
                .p_obj
                .get_object_events_from_beginning(CredentialsDescriptor::Device)
                .await?;
            assert_eq!(device_creds.len(), 1);

            let user_creds = self
                .p_obj
                .get_object_events_from_beginning(CredentialsDescriptor::User)
                .await?;

            assert_eq!(user_creds.len(), 1);

            Ok(())
        }

        pub async fn verify_device_creds(&self) -> anyhow::Result<()> {
            let events = self
                .p_obj
                .get_object_events_from_beginning(CredentialsDescriptor::Device)
                .await?;

            assert_eq!(events.len(), 1);

            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::setup_tracing;
    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::vault::vault::VaultName;
    use crate::node::db::repo::persistent_credentials::spec::PersistentCredentialsSpec;
    use tracing_attributes::instrument;

    #[tokio::test]
    #[instrument]
    async fn test_get_or_generate_user_creds() -> anyhow::Result<()> {
        setup_tracing()?;

        let base_fixture = FixtureRegistry::base().await?;
        let creds_repo = base_fixture.state.p_creds.server_p_creds;

        let vault_name = VaultName::test();
        let _ = creds_repo
            .get_or_generate_user_creds(DeviceName::server(), vault_name)
            .await?;

        let spec = PersistentCredentialsSpec {
            p_obj: creds_repo.p_obj.clone(),
        };
        spec.verify_user_creds().await?;

        Ok(())
    }
}
