use crate::base_command::BaseCommand;
use crate::cli_format::CliOutputFormat;
use anyhow::{bail, Result};
use meta_secret_core::crypto::utils::Id48bit;
use meta_secret_core::node::common::model::{ApplicationState, IdString, VaultFullInfo};
use meta_secret_core::node::common::model::secret::{ClaimId, SsClaim};
use meta_secret_core::node::db::actions::recover::RecoveryHandler;
use serde_json::json;

pub struct ShowSecretCommand {
    base: BaseCommand,
    output_format: CliOutputFormat,
}

impl ShowSecretCommand {
    pub fn new(db_name: String, output_format: CliOutputFormat) -> Self {
        Self {
            base: BaseCommand::new(db_name),
            output_format,
        }
    }
    
    pub async fn execute(self, claim_id: String) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;
        
        // Ensure user credentials exist
        self.base.ensure_user_creds(&db_context).await?;
        let user_creds = db_context.p_creds.get_user_creds().await?.unwrap();

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
                        
                        let maybe_claim = member_info.ss_claims.claims.get(&claim_id);
                        match maybe_claim {
                            None => {
                                bail!("Claim not found.");
                            }
                            Some(claim) => {
                                let handler = RecoveryHandler::from(db_context.p_obj.clone());

                                // Clone claim_id to avoid ownership issues
                                let claim_id_for_recovery = claim_id.clone();
                                let secret = handler
                                    .recover(
                                        user_creds.vault_name.clone(), 
                                        user_creds, 
                                        claim_id_for_recovery, 
                                        claim.dist_claim_id.pass_id.clone()
                                    )
                                    .await?;

                                match self.output_format {
                                    CliOutputFormat::Json => {
                                        let result = json!({
                                            "secret": secret.text,
                                            "status": "success",
                                            "claim_id": claim_id.0.id_str(),
                                            "password_name": claim.dist_claim_id.pass_id.name
                                        });
                                        println!("{}", serde_json::to_string_pretty(&result)?);
                                    },
                                    CliOutputFormat::Yaml => {
                                        println!("secret: {}", secret.text);
                                        println!("status: success");
                                        println!("claim_id: {}", claim_id.0.id_str());
                                        println!("password_name: {}", claim.dist_claim_id.pass_id.name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
} 