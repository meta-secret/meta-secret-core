use std::sync::Arc;

use crate::node::common::model::user::common::{UserData, UserDataOutsiderStatus};
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::node::{
    common::model::vault::vault::VaultStatus,
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

pub struct SignUpClaim<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> SignUpClaim<Repo> {
    pub async fn sign_up(&self, user_data: UserData) -> anyhow::Result<VaultStatus> {
        let creds_repo = PersistentCredentials {
            p_obj: self.p_obj.clone(),
        };
        let p_device_log = PersistentDeviceLog {
            p_obj: self.p_obj.clone(),
        };
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };

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
            VaultStatus::Outsider(outsider) => match outsider.status {
                UserDataOutsiderStatus::NonMember => {
                    p_device_log.save_join_request(&outsider.user_data).await?;
                }
                UserDataOutsiderStatus::Pending => {
                    info!("Device is pending")
                }
                UserDataOutsiderStatus::Declined => {
                    info!("Device has been declined")
                }
            },
            VaultStatus::Member { .. } => {
                //trace!("User is already a vault member: {:?}", member);
            }
        }

        Ok(vault_status.clone())
    }
}

#[cfg(test)]
pub mod test_action {
    use crate::node::common::model::user::user_creds::UserCredentials;
    use crate::node::common::model::vault::vault::VaultStatus;
    use crate::node::db::actions::sign_up::claim::SignUpClaim;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use std::sync::Arc;
    use tracing::info;
    use tracing_attributes::instrument;

    pub struct SignUpClaimTestAction {
        _sign_up: SignUpClaim<InMemKvLogEventRepo>,
    }

    impl SignUpClaimTestAction {
        #[instrument(skip_all)]
        pub async fn sign_up(
            p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,
            creds: &UserCredentials,
        ) -> anyhow::Result<VaultStatus> {
            info!("SignUp action");

            let sign_up_claim = SignUpClaim {
                p_obj: p_obj.clone(),
            };

            let status = sign_up_claim.sign_up(creds.user()).await?;

            Ok(status)
        }
    }
}

#[cfg(test)]
pub mod spec {
    use crate::meta_tests::spec::test_spec::TestSpec;
    use crate::node::common::model::user::common::UserData;
    use crate::node::db::objects::persistent_device_log::spec::DeviceLogSpec;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::objects::persistent_vault::spec::VaultLogSpec;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use anyhow::Result;
    use async_trait::async_trait;
    use log::info;
    use std::sync::Arc;
    use tracing_attributes::instrument;

    pub struct SignUpClaimSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData,
    }

    #[async_trait(? Send)]
    impl<Repo: KvLogEventRepo> TestSpec for SignUpClaimSpec<Repo> {
        #[instrument(skip(self))]
        async fn verify(&self) -> Result<()> {
            info!("SignUp claim spec");

            let device_log_spec = DeviceLogSpec {
                p_obj: self.p_obj.clone(),
                user: self.user.clone(),
            };

            device_log_spec.check_sign_up_request().await?;

            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use anyhow::{bail, Result};

    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::spec::test_spec::TestSpec;
    use crate::node::common::model::vault::vault::VaultStatus;
    use crate::node::db::actions::sign_up::claim::spec::SignUpClaimSpec;
    use crate::node::db::actions::sign_up::claim::test_action::SignUpClaimTestAction;

    #[tokio::test]
    #[ignore]
    async fn test_sign_up() -> Result<()> {
        let registry = FixtureRegistry::extended().await?;
        let p_obj = registry.state.base.empty.p_obj.client.clone();
        let creds = registry.state.base.empty.user_creds;

        let vault_status = SignUpClaimTestAction::sign_up(p_obj.clone(), &creds.client).await?;

        let VaultStatus::Outsider(outsider) = vault_status else {
            bail!("Invalid state: {:?}", vault_status);
        };

        let db = p_obj.repo.get_db().await;
        assert_eq!(db.len(), 8);

        let claim_spec = SignUpClaimSpec {
            p_obj,
            user: outsider.user_data,
        };
        claim_spec.verify().await?;

        Ok(())
    }
}
