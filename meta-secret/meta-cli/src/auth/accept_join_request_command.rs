use crate::base_command::BaseCommand;
use anyhow::{bail, Result};
use meta_secret_core::node::common::model::{ApplicationState, IdString, VaultFullInfo};
use meta_secret_core::node::db::actions::sign_up::join::JoinActionUpdate;
use meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent;
use tracing::info;

pub struct AcceptJoinRequestCommand {
    pub base: BaseCommand,
    pub device_id: String,
}

impl AcceptJoinRequestCommand {
    pub fn new(db_name: String, device_id: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
            device_id,
        }
    }

    pub async fn execute(&self) -> Result<()> {
        info!("Accepting join request for device ID: {}", self.device_id);

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
                    info!("Find join request for device ID: {}", self.device_id);
                    let found_join_request = member_info
                        .vault_events
                        .requests
                        .iter()
                        .filter_map(|request| {
                            if let VaultActionRequestEvent::JoinCluster(join_request) = request {
                                // Compare device ID strings
                                let vault_request_join_id =
                                    join_request.candidate.device.device_id.clone().id_str();

                                if vault_request_join_id == self.device_id {
                                    Some(join_request.clone())
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .next();

                    match found_join_request {
                        Some(join_request) => {
                            info!("Accept the join request");
                            client
                                .update_membership(join_request, JoinActionUpdate::Accept)
                                .await?;
                            println!(
                                "Join request for device {} accepted successfully",
                                self.device_id
                            );
                            Ok(())
                        }
                        None => {
                            bail!("No join request found for device ID: {}", self.device_id);
                        }
                    }
                }
            },
        }
    }
}
