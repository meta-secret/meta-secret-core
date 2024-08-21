use std::sync::Arc;

use anyhow::{bail, Ok};

use crate::node::{
    common::model::{
        user::{UserData, UserDataOutsiderStatus},
        vault::VaultStatus,
    },
    db::{
        events::local_event::CredentialsObject,
        objects::{
            persistent_device_log::PersistentDeviceLog, persistent_object::PersistentObject,
            persistent_shared_secret::PersistentSharedSecret, persistent_vault::PersistentVault,
        },
        repo::{credentials_repo::CredentialsRepo, generic_db::KvLogEventRepo},
    },
};

pub struct SignUpClaim<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> SignUpClaim<Repo> {
    pub async fn sign_up(&self) -> anyhow::Result<VaultStatus> {
        let (user, vault_status) = self.get_vault_status().await?;

        let VaultStatus::Outsider(outsider) = &vault_status else {
            return Ok(vault_status);
        };

        let UserDataOutsiderStatus::Unknown = &outsider.status else {
            return Ok(vault_status);
        };

        let p_device_log = PersistentDeviceLog {
            p_obj: self.p_obj.clone(),
        };

        p_device_log.save_create_vault_request(&user).await?;

        //Init SSDeviceLog
        let p_ss = PersistentSharedSecret {
            p_obj: self.p_obj.clone(),
        };
        p_ss.init(user.clone()).await?;

        Ok(vault_status)
    }

    pub async fn get_vault_status(&self) -> anyhow::Result<(UserData, VaultStatus)> {
        let creds = {
            let creds_repo = CredentialsRepo {
                p_obj: self.p_obj.clone(),
            };
            creds_repo.get().await?
        };

        match creds {
            CredentialsObject::Device { .. } => {
                bail!("User credentials not found")
            }
            CredentialsObject::DefaultUser(event) => {
                let user = event.value.user();
                //get vault status, if not member, then create request to join
                let p_vault = PersistentVault {
                    p_obj: self.p_obj.clone(),
                };

                let vault_status = p_vault.find(user.clone()).await?;
                Ok((user, vault_status))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use anyhow::{bail, Result};
    use std::sync::Arc;

    use crate::{
        meta_tests::{
            action::sign_up_claim_action::SignUpClaimTestAction,
            spec::{sign_up_claim_spec::SignUpClaimSpec, test_spec::TestSpec},
        },
        node::{
            common::model::vault::VaultStatus,
            db::{in_mem_db::InMemKvLogEventRepo, objects::persistent_object::PersistentObject},
        },
    };

    #[tokio::test]
    async fn test_sign_up() -> Result<()> {
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let p_obj = Arc::new(PersistentObject::new(repo.clone()));

        let claim_action = SignUpClaimTestAction::new(p_obj.clone());
        let vault_status = claim_action.sign_up().await?;
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
