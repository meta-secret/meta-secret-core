use std::sync::Arc;

use crate::node::common::model::user::common::UserData;
use crate::node::{
    common::model::vault::VaultStatus,
    db::{
        objects::{
            persistent_device_log::PersistentDeviceLog, persistent_object::PersistentObject,
            persistent_vault::PersistentVault,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::Ok;
use log::info;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;

pub struct SignUpClaim<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> SignUpClaim<Repo> {
    pub async fn sign_up(&self, user_data: UserData) -> anyhow::Result<VaultStatus> {
        let creds_repo = PersistentCredentials { p_obj: self.p_obj.clone() };
        let p_device_log = PersistentDeviceLog { p_obj: self.p_obj.clone() };
        let p_vault = PersistentVault { p_obj: self.p_obj.clone(), };

        let device_name = user_data.device.device_name.clone();
        let vault_name = user_data.vault_name.clone();
        creds_repo
            .get_or_generate_user_creds(device_name, vault_name)
            .await?;

        let vault_status = p_vault.find(user_data.clone()).await?;

        match &vault_status {
            VaultStatus::NotExists(user_data) => {
                info!("Vault doesn't exists");
                p_device_log.save_create_vault_request(user_data).await?;
            }
            VaultStatus::Outsider(outsider) => {
                if outsider.is_non_member() {
                    p_device_log.save_join_request(&outsider.user_data).await?;
                } else {
                    info!("Device is pending or declined")
                }
            }
            VaultStatus::Member { .. } => {
                //trace!("User is already a vault member: {:?}", member);
            }
        }
        
        let p_ss_device_log = PersistentSharedSecret {
            p_obj: self.p_obj.clone(),
        };

        p_ss_device_log.init(user_data).await?;

        Ok(vault_status.clone())
    }
}

#[cfg(test)]
pub mod test_action {
    use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
    use crate::node::common::model::vault::VaultStatus;
    use crate::node::db::actions::sign_up::claim::SignUpClaim;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use std::sync::Arc;
    use tracing::info;
    use tracing_attributes::instrument;

    pub struct SignUpClaimTestAction {
        sign_up: SignUpClaim<InMemKvLogEventRepo>,
    }

    impl SignUpClaimTestAction {
        
        #[instrument(skip_all)]
        pub async fn sign_up(
            p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>, 
            creds_fixture: &UserCredentialsFixture
        ) -> anyhow::Result<VaultStatus> {
            info!("SignUp action");

            let sign_up_claim = SignUpClaim {
                p_obj: p_obj.clone(),
            };

            let status = sign_up_claim.sign_up(creds_fixture.client.user()).await?;
            
            Ok(status)
        }
    }
}

#[cfg(test)]
pub mod spec {
    use std::sync::Arc;
    use async_trait::async_trait;
    use crate::meta_tests::spec::test_spec::TestSpec;
    use crate::node::common::model::user::common::UserData;
    use crate::node::db::objects::persistent_device_log::spec::DeviceLogSpec;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::objects::persistent_shared_secret::spec::SSDeviceLogSpec;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use anyhow::Result;
    use log::info;
    use tracing_attributes::instrument;
    use crate::node::common::model::device::common::DeviceData;
    use crate::node::db::objects::global_index::spec::GlobalIndexSpec;
    use crate::node::db::objects::persistent_vault::spec::VaultLogSpec;

    pub struct SignUpClaimSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData,
        pub server_device: DeviceData
    }

    #[async_trait(? Send)]
    impl<Repo: KvLogEventRepo> TestSpec for SignUpClaimSpec<Repo> {
        #[instrument(skip(self))]
        async fn verify(&self) -> Result<()> {
            info!("SignUp claim spec");

            let gi_spec = GlobalIndexSpec { 
                repo: self.p_obj.repo.clone(), 
                server_device: self.server_device.clone() 
            };
            gi_spec.verify().await?;
            
            let device_log_spec = DeviceLogSpec {
                p_obj: self.p_obj.clone(),
                user: self.user.clone(),
            };

            device_log_spec.check_initialization().await?;
            device_log_spec.check_sign_up_request().await?;
            
            let vault_log_spec = VaultLogSpec {
                p_obj: self.p_obj.clone(),
                user: self.user.clone(),
            };

            //vault_log_spec.verify_initial_state().await?;

            let ss_device_log_spec = SSDeviceLogSpec {
                p_obj: self.p_obj.clone(),
                client_user: self.user.clone(),
            };

            ss_device_log_spec.check_initialization().await?;

            Ok(())
        }
    }

}

#[cfg(test)]
mod test {
    use anyhow::{bail, Result};

    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::db::actions::sign_up::claim::test_action::SignUpClaimTestAction;
    use crate::{node::common::model::vault::VaultStatus};
    use crate::meta_tests::spec::test_spec::TestSpec;
    use crate::node::db::actions::sign_up::claim::spec::SignUpClaimSpec;

    #[tokio::test]
    async fn test_sign_up() -> Result<()> {
        let registry = FixtureRegistry::extended().await?;
        let p_obj = registry.state.base.empty.p_obj.client.clone();
        let creds = registry.state.base.empty.user_creds;
        let vault_status = SignUpClaimTestAction::sign_up(p_obj.clone(), &creds)
            .await?;
        
        let VaultStatus::Outsider(outsider) = vault_status else {
            bail!("Invalid state");
        };

        let db = p_obj.repo.get_db().await;
        assert_eq!(db.len(), 8);

        let claim_spec = SignUpClaimSpec {
            p_obj,
            user: outsider.user_data,
            server_device: registry.state.base.empty.device_creds.server.device,
        };
        claim_spec.verify().await?;

        Ok(())
    }
}
