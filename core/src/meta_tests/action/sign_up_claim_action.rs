use anyhow::Result;
use std::sync::Arc;
use tracing_attributes::instrument;
use crate::node::{
    common::model::vault::VaultStatus,
    db::{
        actions::sign_up_claim::SignUpClaim, objects::persistent_object::PersistentObject,
        repo::generic_db::KvLogEventRepo,
    },
};

use super::generate_user::GenerateUserTestAction;

pub struct SignUpClaimTestAction<Repo: KvLogEventRepo> {
    p_obj: Arc<PersistentObject<Repo>>,
    generate_user_action: Arc<GenerateUserTestAction<Repo>>,
}

impl<Repo: KvLogEventRepo> SignUpClaimTestAction<Repo> {
    pub fn new(p_obj: Arc<PersistentObject<Repo>>) -> Self {
        Self {
            p_obj: p_obj.clone(),
            generate_user_action: Arc::new(GenerateUserTestAction::new(p_obj)),
        }
    }
}

impl<Repo: KvLogEventRepo> SignUpClaimTestAction<Repo> {
    
    #[instrument(skip_all)]
    pub async fn sign_up(&self) -> Result<VaultStatus> {
        let user_creds = self.generate_user_action.generate_user().await?;

        let sign_up_claim = SignUpClaim {
            p_obj: self.p_obj.clone(),
        };

        sign_up_claim.sign_up(user_creds.user()).await
    }
}
