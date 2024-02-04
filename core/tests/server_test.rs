#[cfg(test)]
mod test {
    use std::sync::Arc;

    use meta_secret_core::{
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
    pub async fn test_server_app_initial_state() -> anyhow::Result<()> {
        let client_p_obj = {
            let client_repo = Arc::new(InMemKvLogEventRepo::default());
            Arc::new(PersistentObject::new(client_repo.clone()))
        };

        let claim_action = SignUpClaimTestAction::new(client_p_obj.clone());
        let vault_status = claim_action.sign_up().await?;

        let VaultStatus::Outsider(outsider) = vault_status else {
            panic!("Invalid state");
        };

        let claim_spec = SignUpClaimSpec {
            p_obj: client_p_obj,
            user: outsider.user_data,
        };

        claim_spec.check().await?;

        Ok(())
    }
}
