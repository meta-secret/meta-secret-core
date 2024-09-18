use std::sync::Arc;

use crate::node::common::model::user::UserData;
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
