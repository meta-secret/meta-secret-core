use crate::base_command::BaseCommand;
use anyhow::{bail, Result};
use meta_secret_core::crypto::utils::Id48bit;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::common::model::secret::{ClaimId, SsClaim};
use meta_secret_core::node::db::actions::recover::RecoveryHandler;

pub struct ShowSecretCommand {
    base: BaseCommand,
}

impl ShowSecretCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }
    
    pub async fn execute(self, claim_id: String) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;
        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;

        println!();
        let Some(user_creds) = maybe_user_creds else {
            println!("User Status: Device is initialized but not associated with a vault.");
            println!(
                "Run the 'meta-secret init-user --vault-name <n>' command to associate it with a vault."
            );
            return Ok(());
        };

        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        match app_state {
            ApplicationState::Local(_) => {
                bail!("Local state: operation is not possible.");
            }
            ApplicationState::Vault(vault_full_info) => {
                match vault_full_info {
                    VaultFullInfo::NotExists(_) => {
                        bail!("Vault does not exist.");
                    }
                    VaultFullInfo::Outsider(_) => {
                        bail!("Outsider: operation is not possible.");
                    }
                    VaultFullInfo::Member(member_info) => {
                        let claim_id = {
                            let typed_id = Id48bit::from(claim_id);
                            ClaimId::from(typed_id)
                        };
                        
                        let maybe_claim =  member_info.ss_claims.claims.get(&claim_id);
                        match maybe_claim {
                            None => {
                                bail!("Claim not found.");
                            }
                            Some(claim) => {
                                let handler = RecoveryHandler::from(db_context.p_obj.clone());

                                let secret = handler
                                    .recover(
                                        user_creds.vault_name.clone(), 
                                        user_creds, 
                                        claim_id, 
                                        claim.dist_claim_id.pass_id.clone()
                                    )
                                    .await?;

                                println!("May the Meta be with you...");
                                println!("{}", secret.text)
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
}