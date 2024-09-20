use std::sync::Arc;

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
use tracing::warn;
use crate::node::common::model::user::common::UserData;

pub struct SignUpClaim<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> SignUpClaim<Repo> {
    pub async fn sign_up(&self, user_data: UserData) -> anyhow::Result<VaultStatus> {
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };

        let vault_status = p_vault.find(user_data).await?;

        let p_device_log = PersistentDeviceLog {
            p_obj: self.p_obj.clone(),
        };
        match &vault_status {
            VaultStatus::NotExists(user_data) => {
                p_device_log.save_create_vault_request(user_data).await?;
            }
            VaultStatus::Outsider(outsider) => {
                if outsider.is_non_member() {
                    p_device_log.save_join_request(&outsider.user_data).await?;
                }
            }
            VaultStatus::Member { member, .. } => {
                warn!("User: {:?} is already vault member", member);
            }
        }

        Ok(vault_status.clone())
    }
}

#[cfg(test)]
pub mod test_action {
    use std::sync::Arc;
    use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
    use crate::node::common::model::vault::VaultStatus;
    use crate::node::db::actions::sign_up_claim::SignUpClaim;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::repo::credentials_repo::CredentialsRepo;

    pub struct SignUpClaimTestAction {
        sign_up: SignUpClaim<InMemKvLogEventRepo>,
    }

    impl SignUpClaimTestAction {
        pub async fn sign_up(p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>, creds_fixture: UserCredentialsFixture) -> anyhow::Result<VaultStatus> {
            let creds_repo = CredentialsRepo { p_obj: p_obj.clone() };

            let device_name = creds_fixture.client_device_name();
            let vault_name = creds_fixture.client.vault_name.clone();
            creds_repo
                .get_or_generate_user_creds(device_name, vault_name)
                .await?;

            let sign_up_claim = SignUpClaim {
                p_obj: p_obj.clone(),
            };

            let status = sign_up_claim.sign_up(creds_fixture.client.user()).await?;
            
            Ok(status)
        }
    }
}

#[cfg(test)]
mod test {
    use anyhow::{bail, Result};
    use std::sync::Arc;

    use crate::{
        meta_tests::{
            spec::{sign_up_claim_spec::SignUpClaimSpec, test_spec::TestSpec},
        },
        node::{
            common::model::vault::VaultStatus,
            db::{in_mem_db::InMemKvLogEventRepo, objects::persistent_object::PersistentObject},
        },
    };
    use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
    use crate::node::db::actions::sign_up_claim::test_action::SignUpClaimTestAction;

    #[tokio::test]
    async fn test_sign_up() -> Result<()> {
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));

        let vault_status = SignUpClaimTestAction::sign_up(p_obj.clone(), UserCredentialsFixture::generate())
            .await?;
        let VaultStatus::Outsider(outsider) = vault_status else {
            bail!("Invalid state");
        };

        let db = repo.get_db().await;
        assert_eq!(db.len(), 8);

        let claim_spec = SignUpClaimSpec {
            p_obj,
            user: outsider.user_data,
        };
        claim_spec.verify().await?;

        Ok(())
    }
}
