use crate::crypto::keys::{TransportSk};
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::device::device_creds::{
    DeviceCreds, DeviceCredsBuilder, SecureDeviceCreds,
};
use crate::node::common::model::user::user_creds::{SecureUserCreds, UserCreds, UserCredsBuilder};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::descriptors::creds::{DeviceCredsDescriptor, UserCredsDescriptor};
use crate::node::db::events::local_event::{DeviceCredsObject, UserCredsObject};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{Result, bail};
use derive_more::From;
use std::sync::Arc;
use tracing::{info, instrument};

#[derive(From)]
pub struct PersistentCredentials<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub master_key: TransportSk,
}

impl<Repo: KvLogEventRepo> PersistentCredentials<Repo> {
    pub async fn get_device_creds(&self) -> Result<Option<DeviceCreds>> {
        let maybe_secure_device_creds_obj =
            self.p_obj.find_tail_event(DeviceCredsDescriptor).await?;

        match maybe_secure_device_creds_obj {
            None => Ok(None),
            Some(secure_device_creds_obj) => {
                let device_creds = secure_device_creds_obj.value().decrypt(&self.master_key)?;
                Ok(Some(device_creds))
            }
        }
    }

    #[instrument(skip(self))]
    pub async fn get_or_generate_device_creds(
        &self,
        device_name: DeviceName,
    ) -> Result<DeviceCreds> {
        let maybe_device_creds = self.get_device_creds().await?;

        let device_creds = match maybe_device_creds {
            None => self.generate_device_creds(device_name).await?,
            Some(creds) => creds,
        };
        Ok(device_creds)
    }

    #[instrument(skip(self))]
    pub async fn save_device_creds(&self, device_creds: DeviceCreds) -> Result<()> {
        let secure_creds = SecureDeviceCreds::try_from(device_creds.clone())?;
        let creds_obj = DeviceCredsObject::from(secure_creds);
        self.p_obj.repo.save(creds_obj).await?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn save_user_creds(&self, user_creds: UserCreds) -> Result<ArtifactId> {
        // Convert device credentials to secure device credentials
        let secure_device_creds = SecureDeviceCreds::try_from(user_creds.device_creds.clone())?;
        
        // Create secure user credentials with the secure device credentials
        let secure_user_creds = SecureUserCreds {
            vault_name: user_creds.vault_name.clone(),
            device_creds: secure_device_creds,
        };
        
        // Create a user credentials object and save it
        let creds_obj = UserCredsObject::from(secure_user_creds);
        self.p_obj.repo.save(creds_obj).await
    }

    #[instrument(skip_all)]
    pub async fn get_user_creds(&self) -> Result<Option<UserCreds>> {
        let maybe_secure_user_creds_obj = self
            .p_obj
            .find_tail_event(UserCredsDescriptor)
            .await?;

        match maybe_secure_user_creds_obj {
            None => Ok(None),
            Some(secure_user_creds_obj) => {
                let secure_user_creds = secure_user_creds_obj.value();
                
                // Decrypt device credentials
                let device_creds = secure_user_creds.device_creds.decrypt(&self.master_key)?;
                
                // Create UserCreds with the decrypted device credentials
                let user_creds = UserCreds {
                    vault_name: secure_user_creds.vault_name,
                    device_creds,
                };
                
                Ok(Some(user_creds))
            }
        }
    }

    #[instrument(skip(self))]
    async fn generate_device_creds(&self, device_name: DeviceName) -> Result<DeviceCreds> {
        let device_creds = DeviceCredsBuilder::generate().build(device_name).creds;
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
    ) -> Result<UserCreds> {
        let device_creds = self.get_or_generate_device_creds(device_name.clone()).await?;

        let maybe_user_creds = self.get_user_creds().await?;

        let user_creds = match maybe_user_creds {
            None => {
                let user_creds = UserCredsBuilder::init(device_creds.clone())
                    .build(vault_name)
                    .creds;
                self.save_user_creds(user_creds.clone()).await?;
                user_creds
            }
            Some(creds) => creds,
        };

        if !user_creds.device_creds.device.device_id.eq(&device_creds.device.device_id) {
            bail!("Inconsistent credentials: device credentials do not match user credentials");
        }

        Ok(user_creds)
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
                master_key: state.device_creds.client_master_key.clone(),
            });

            let vd_p_creds = Arc::new(PersistentCredentials {
                p_obj: state.p_obj.vd.clone(),
                master_key: state.device_creds.vd_master_key.clone(),
            });

            let server_p_creds = Arc::new(PersistentCredentials {
                p_obj: state.p_obj.server.clone(),
                master_key: state.device_creds.server_master_key.clone(),
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
    use crate::node::db::descriptors::creds::{DeviceCredsDescriptor, UserCredsDescriptor};
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
                .get_object_events_from_beginning(DeviceCredsDescriptor)
                .await?;
            assert_eq!(device_creds.len(), 1);

            let user_creds = self
                .p_obj
                .get_object_events_from_beginning(UserCredsDescriptor)
                .await?;

            assert_eq!(user_creds.len(), 1);

            Ok(())
        }

        pub async fn verify_device_creds(&self) -> anyhow::Result<()> {
            let events = self
                .p_obj
                .get_object_events_from_beginning(DeviceCredsDescriptor)
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
    use crate::crypto::keys::TransportSk;
    use crate::crypto::key_pair::{KeyPair, TransportDsaKeyPair};
    use std::sync::Arc;
    use crate::node::db::objects::persistent_object::PersistentObject;

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

    #[tokio::test]
    #[instrument]
    async fn test_credentials_encryption_decryption() -> anyhow::Result<()> {
        setup_tracing()?;

        let base_fixture = FixtureRegistry::base().await?;
        
        // Generate a transport key for encryption/decryption
        let transport_key_pair = TransportDsaKeyPair::generate();
        let master_key = transport_key_pair.sk();
        
        // Create a new PersistentCredentials with our master key
        let creds_repo = Arc::new(super::PersistentCredentials {
            p_obj: base_fixture.state.p_creds.client_p_creds.p_obj.clone(),
            master_key,
        });

        // Generate device and user credentials
        let device_name = DeviceName::client();
        let vault_name = VaultName::test();
        
        // Generate and save device credentials
        let device_creds = creds_repo.generate_device_creds(device_name.clone()).await?;
        
        // Create and save user credentials
        let user_creds = crate::node::common::model::user::user_creds::UserCredsBuilder::init(device_creds.clone())
            .build(vault_name.clone())
            .creds;
        creds_repo.save_user_creds(user_creds.clone()).await?;
        
        // Retrieve device and user credentials
        let retrieved_device_creds = creds_repo.get_device_creds().await?.unwrap();
        let retrieved_user_creds = creds_repo.get_user_creds().await?.unwrap();
        
        // Verify device credentials match
        assert_eq!(device_creds.device.device_id, retrieved_device_creds.device.device_id);
        assert_eq!(device_creds.device.device_name, retrieved_device_creds.device.device_name);
        
        // Verify user credentials match
        assert_eq!(user_creds.vault_name, retrieved_user_creds.vault_name);
        assert_eq!(user_creds.device_creds.device.device_id, retrieved_user_creds.device_creds.device.device_id);
        
        Ok(())
    }
}
