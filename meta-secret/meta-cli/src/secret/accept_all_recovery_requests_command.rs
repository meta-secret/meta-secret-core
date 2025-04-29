use crate::base_command::BaseCommand;
use anyhow::{bail, Result};
use tracing::info;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::common::model::secret::SecretDistributionType;

pub struct AcceptAllRecoveryRequestsCommand {
    pub base: BaseCommand,
}

impl AcceptAllRecoveryRequestsCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    pub async fn execute(&self) -> Result<()> {
        info!("Accepting all pending recovery requests");
        
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
                    info!("Finding all pending recovery requests");
                    
                    // Filter recovery requests from ss_claims
                    let recovery_requests: Vec<_> = member_info
                        .ss_claims
                        .claims
                        .iter()
                        .filter(|(_, claim)| claim.distribution_type == SecretDistributionType::Recover)
                        .collect();
                    let recovery_requests_num = recovery_requests.len();

                    if recovery_requests.is_empty() {
                        println!("No pending recovery requests found");
                        return Ok(());
                    }

                    println!("Found {} pending recovery requests", recovery_requests.len());
                    
                    let mut accepted_count = 0;
                    for (claim_id, claim) in recovery_requests {
                        info!("Accepting recovery request for password: {}", claim.dist_claim_id.pass_id.name);
                        
                        match client.accept_recover(claim_id.clone()).await {
                            Ok(_) => {
                                println!("Accepted recovery request for password '{}'", claim.dist_claim_id.pass_id.name);
                                accepted_count += 1;
                            }
                            Err(e) => {
                                println!("Failed to accept recovery request for password '{}': {}", 
                                    claim.dist_claim_id.pass_id.name, e);
                            }
                        }
                    }
                    
                    println!("Successfully accepted {} out of {} recovery requests", 
                        accepted_count, recovery_requests_num);
                    Ok(())
                }
            },
        }
    }
} 