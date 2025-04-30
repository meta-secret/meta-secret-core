use crate::base_command::BaseCommand;
use anyhow::{Result, bail};
use meta_secret_core::node::common::model::{ApplicationState, IdString, VaultFullInfo};
use meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent;
use tracing::info;

pub struct AcceptAllJoinRequestsCommand {
    pub base: BaseCommand,
}

impl AcceptAllJoinRequestsCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    pub async fn execute(&self) -> Result<()> {
        info!("Accepting all pending join requests");

        // Open the existing database
        let db_context = self.base.open_existing_db().await?;

        // Create client service
        let client = self.base.create_client_service(&db_context).await?;

        // Get the application state
        let app_state = client.get_app_state().await?;

        // Check if application state is Vault and user is a member
        match app_state {
            ApplicationState::Local(_) => {
                bail!("Invalid state. Local App State cannot accept join requests");
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(_) => {
                    bail!("Invalid state. Vault doesn't exist");
                }
                VaultFullInfo::Outsider(_) => {
                    bail!("Invalid state. User is outsider and cannot accept join requests");
                }
                VaultFullInfo::Member(member_info) => {
                    info!("Finding all pending join requests");
                    let join_requests: Vec<_> = member_info
                        .vault_events
                        .requests
                        .iter()
                        .filter_map(|request| {
                            if let VaultActionRequestEvent::JoinCluster(join_request) = request {
                                Some(join_request.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    if join_requests.is_empty() {
                        println!("No pending join requests found");
                        return Ok(());
                    }

                    println!("Found {} pending join requests", join_requests.len());

                    let mut accepted_count = 0;
                    for join_request in &join_requests {
                        let device_id = join_request.candidate.device.device_id.clone().id_str();
                        info!("Accepting join request for device ID: {}", device_id);

                        match client.accept_join(join_request.clone()).await {
                            Ok(_) => {
                                println!("Accepted join request for device {}", device_id);
                                accepted_count += 1;
                            }
                            Err(e) => {
                                println!(
                                    "Failed to accept join request for device {}: {}",
                                    device_id, e
                                );
                            }
                        }
                    }

                    println!(
                        "Successfully accepted {} out of {} join requests",
                        accepted_count,
                        join_requests.len()
                    );
                    Ok(())
                }
            },
        }
    }
}
