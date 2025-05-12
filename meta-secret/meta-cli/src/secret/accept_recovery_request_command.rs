use crate::base_command::BaseCommand;
use anyhow::{bail, Result};
use meta_secret_core::crypto::utils::Id48bit;
use meta_secret_core::node::common::model::secret::{ClaimId, SecretDistributionType};
use meta_secret_core::node::common::model::{ApplicationState, IdString, VaultFullInfo};
use tracing::info;

pub struct AcceptRecoveryRequestCommand {
    pub base: BaseCommand,
    pub claim_id: String,
}

impl AcceptRecoveryRequestCommand {
    pub fn new(db_name: String, claim_id: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
            claim_id,
        }
    }

    pub async fn execute(&self) -> Result<()> {
        info!(
            "Accepting recovery request with claim ID: {}",
            self.claim_id
        );

        // Open the existing database
        let db_context = self.base.open_existing_db().await?;

        // Create client service
        let client = self.base.create_client_service(&db_context).await?;

        // Get the application state
        let app_state = client.get_app_state().await?;

        // Check if application state is Vault and user is a member
        match app_state {
            ApplicationState::Local(_) => {
                bail!("Invalid state. Local App State cannot accept recovery requests");
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(_) => {
                    bail!("Invalid state. Vault doesn't exist");
                }
                VaultFullInfo::Outsider(_) => {
                    bail!("Invalid state. User is outsider and cannot accept recovery requests");
                }
                VaultFullInfo::Member(member_info) => {
                    // Convert string claim ID to ClaimId type
                    let typed_id = Id48bit::from(self.claim_id.clone());
                    let claim_id = ClaimId::from(typed_id.clone());

                    // Find the recovery request for the given claim ID
                    let found_recovery_request =
                        member_info.ss_claims.claims.iter().find(|(id, claim)| {
                            // Check if this is a recovery claim with matching ID
                            id.0.clone().id_str() == typed_id.clone().id_str()
                                && claim.distribution_type == SecretDistributionType::Recover
                        });

                    match found_recovery_request {
                        Some((_, claim)) => {
                            info!(
                                "Found recovery request for password: {}",
                                claim.dist_claim_id.pass_id.name
                            );
                            info!("Accepting the recovery request");

                            client.accept_recover(claim_id).await?;

                            println!(
                                "Recovery request for password '{}' accepted successfully",
                                claim.dist_claim_id.pass_id.name
                            );
                            Ok(())
                        }
                        None => {
                            bail!("No recovery request found for claim ID: {}", self.claim_id);
                        }
                    }
                }
            },
        }
    }
}
