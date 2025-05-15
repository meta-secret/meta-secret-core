use std::sync::Arc;

use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::user::common::{UserData, UserDataOutsiderStatus};
use crate::node::common::model::user::user_creds::UserCreds;
use crate::node::common::model::vault::vault::VaultName;
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
use derive_more::From;
use tracing::info;
use tracing_attributes::instrument;
use crate::crypto::keys::TransportSk;

#[derive(From)]
pub struct SignUpClaim<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> SignUpClaim<Repo> {
    pub async fn prepare_sign_up(
        &self,
        device_name: DeviceName,
        vault_name: VaultName,
        master_key: TransportSk,
    ) -> anyhow::Result<UserCreds> {
        let creds_repo = PersistentCredentials {
            p_obj: self.p_obj.clone(),
            master_key
        };
        creds_repo
            .get_or_generate_user_creds(device_name, vault_name)
            .await
    }

    #[instrument(skip(self))]
    pub async fn sign_up(&self, user_data: UserData) -> anyhow::Result<VaultStatus> {
        info!("Sign up action");

        let p_device_log = PersistentDeviceLog::from(self.p_obj.clone());
        let p_vault = PersistentVault::from(self.p_obj.clone());

        let vault_status = p_vault.find(user_data.clone()).await?;

        match &vault_status {
            VaultStatus::NotExists(user_data) => {
                info!("Vault doesn't exists");
                p_device_log.save_create_vault_request(user_data).await?;
            }
            VaultStatus::Outsider(outsider) => match outsider.status {
                UserDataOutsiderStatus::NonMember => {
                    info!("Save Join request");
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

#[cfg(any(test, feature = "test-framework"))]
pub mod test_action {
    use crate::node::common::model::user::user_creds::UserCreds;
    use crate::node::common::model::vault::vault::VaultStatus;
    use crate::node::db::actions::sign_up::claim::SignUpClaim;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use std::sync::Arc;
    use tracing::info;
    use tracing_attributes::instrument;
    use crate::crypto::keys::TransportSk;

    pub struct SignUpClaimTestAction {
        _sign_up: SignUpClaim<InMemKvLogEventRepo>,
    }

    impl SignUpClaimTestAction {
        #[instrument(skip_all)]
        pub async fn sign_up(
            p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,
            creds: &UserCreds,
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

#[cfg(any(test, feature = "test-framework"))]
pub mod spec {
    use crate::meta_tests::spec::test_spec::TestSpec;
    use crate::node::common::model::user::common::UserData;
    use crate::node::db::objects::persistent_device_log::spec::DeviceLogSpec;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use anyhow::Result;
    use async_trait::async_trait;
    use std::sync::Arc;
    use tracing::info;
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
