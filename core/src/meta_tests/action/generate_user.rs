use anyhow::Result;
use std::sync::Arc;
use tracing_attributes::instrument;
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

    #[instrument(skip_all)]
    pub async fn generate_user(&self) -> Result<UserCredentials> {
        let device_name = DeviceName::from("client");
        let vault_name = VaultName::from("test_vault");
        self.creds_repo
            .get_or_generate_user_creds(device_name, vault_name)
            .await
    }
}
