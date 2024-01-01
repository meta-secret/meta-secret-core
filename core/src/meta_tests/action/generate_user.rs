use anyhow::Result;
use std::sync::Arc;

use crate::node::{
    common::model::{device::DeviceName, user::UserCredentials, vault::VaultName},
    db::{
        objects::persistent_object::PersistentObject,
        repo::{credentials_repo::CredentialsRepo, generic_db::KvLogEventRepo},
    },
};

pub struct GenerateUserTestAction<Repo: KvLogEventRepo> {
    creds_repo: Arc<CredentialsRepo<Repo>>,
}

impl<Repo: KvLogEventRepo> GenerateUserTestAction<Repo> {
    pub fn new(p_obj: Arc<PersistentObject<Repo>>) -> Self {
        let creds_repo = Arc::new(CredentialsRepo { p_obj });
        Self { creds_repo }
    }

    pub async fn generate_user(&self) -> Result<UserCredentials> {
        self.creds_repo
            .get_or_generate_user_creds(DeviceName::from("client"), VaultName::from("test_vault"))
            .await
    }
}
