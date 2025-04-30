use crate::base_command::BaseCommand;
use anyhow::Result;
use meta_secret_core::node::common::model::user::common::UserMembership;
use meta_secret_core::node::common::model::{ApplicationState, IdString, VaultFullInfo};
use meta_secret_core::node::common::model::secret::SsDistributionStatus;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent;
use serde_json::json;

pub struct InfoCommand {
    base: BaseCommand,
}

impl InfoCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    /// Shows recovery claims information in JSON format
    pub async fn show_recovery_claims(&self) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;
        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        match app_state {
            ApplicationState::Vault(VaultFullInfo::Member(member_info)) => {
                if member_info.ss_claims.claims.is_empty() {
                    println!("{{\"claims\": []}}");
                    return Ok(());
                }

                let mut claims_json = Vec::new();
                for (claim_id, ss_claim) in &member_info.ss_claims.claims {
                    let mut receivers = Vec::new();
                    for receiver in &ss_claim.receivers {
                        let status = ss_claim.status.get(receiver)
                            .map_or("Unknown", |s| match s {
                                SsDistributionStatus::Pending => "Pending",
                                SsDistributionStatus::Sent => "Sent",
                                SsDistributionStatus::Delivered => "Delivered",
                            });
                        
                        receivers.push(json!({
                            "id": receiver.clone().id_str(),
                            "status": status
                        }));
                    }

                    claims_json.push(json!({
                        "id": claim_id.0.clone().id_str(),
                        "sender": ss_claim.sender.clone().id_str(),
                        "type": format!("{:?}", ss_claim.distribution_type),
                        "password": ss_claim.dist_claim_id.pass_id.name.clone(),
                        "status": format!("{:?}", ss_claim.status.status()),
                        "receivers": receivers
                    }));
                }

                let output = json!({
                    "claims": claims_json
                });

                println!("{}", serde_json::to_string_pretty(&output)?);
            },
            _ => {
                println!("{{\"error\": \"Not a vault member or vault doesn't exist\"}}");
            }
        }

        Ok(())
    }

    /// Shows secrets information in JSON format
    pub async fn show_secrets(&self) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;
        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        match app_state {
            ApplicationState::Vault(VaultFullInfo::Member(member_info)) => {
                if member_info.member.vault.secrets.is_empty() {
                    println!("{{\"secrets\": []}}");
                    return Ok(());
                }

                let mut secrets_json = Vec::new();
                for secret_id in &member_info.member.vault.secrets {
                    secrets_json.push(json!({
                        "id": secret_id.id.clone().id_str(),
                        "name": secret_id.name.clone()
                    }));
                }

                let output = json!({
                    "secrets": secrets_json
                });

                println!("{}", serde_json::to_string_pretty(&output)?);
            },
            _ => {
                println!("{{\"error\": \"Not a vault member or vault doesn't exist\"}}");
            }
        }

        Ok(())
    }

    /// Shows vault events information in JSON format
    pub async fn show_vault_events(&self) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;
        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        match app_state {
            ApplicationState::Vault(VaultFullInfo::Member(member_info)) => {
                if member_info.vault_events.requests.is_empty() {
                    println!("{{\"events\": []}}");
                    return Ok(());
                }

                let mut events_json = Vec::new();
                for request in &member_info.vault_events.requests {
                    match request {
                        VaultActionRequestEvent::JoinCluster(join_request) => {
                            events_json.push(json!({
                                "type": "JoinCluster",
                                "device_name": join_request.candidate.device.device_name.as_str().to_string(),
                                "user_id": format!("{:?}", join_request.candidate.user_id())
                            }));
                        },
                        VaultActionRequestEvent::AddMetaPass(meta_pass) => {
                            events_json.push(json!({
                                "type": "AddMetaPass",
                                "meta_pass_id": format!("{:?}", meta_pass.meta_pass_id),
                                "sender": format!("{:?}", meta_pass.sender.user_data.user_id())
                            }));
                        },
                    }
                }

                let output = json!({
                    "events": events_json
                });

                println!("{}", serde_json::to_string_pretty(&output)?);
            },
            _ => {
                println!("{{\"error\": \"Not a vault member or vault doesn't exist\"}}");
            }
        }

        Ok(())
    }

    pub async fn execute(&self) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;

        println!("Meta Secret Information:");
        println!("------------------------");

        // Try to get device credentials
        let maybe_device_creds = db_context.p_creds.get_device_creds().await?;

        match maybe_device_creds {
            Some(device_creds_event) => {
                let device_creds = device_creds_event.value();
                println!("Device Information:");
                println!("  Device ID: {}", device_creds.device.device_id);
                println!(
                    "  Device Name: {}",
                    device_creds.device.device_name.as_str()
                );
            }
            None => {
                println!("Not initialized. Run the 'meta-secret init-device' command first.");
                return Ok(());
            }
        }

        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;

        println!();
        let Some(user_creds) = maybe_user_creds else {
            println!("User Status: Device is initialized but not associated with a vault.");
            println!(
                "Run the 'meta-secret init-user --vault-name <n>' command to associate it with a vault."
            );
            return Ok(());
        };

        println!("User Information:");
        println!("  Vault Name: {}", user_creds.vault_name);
        println!();

        // Create client service
        println!("Syncing with server to get latest state...");
        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        println!("Application State:");
        match app_state {
            ApplicationState::Local(device_data) => {
                println!("  Status: Local");
                println!("  Device is initialized but not connected to a vault");
                println!("  Device ID: {}", device_data.device_id);
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(user_data) => {
                    println!("  Status: Vault not exists");
                    println!("  User has created credentials but the vault doesn't exist yet");
                    println!("  Vault Name: {}", user_data.vault_name());
                }
                VaultFullInfo::Outsider(outsider) => {
                    println!("  Status: Outsider");
                    println!("  User is not a member of the vault");
                    println!("  Vault Name: {}", outsider.user_data.vault_name());
                    println!("  User needs to be invited to join the vault");
                }
                VaultFullInfo::Member(member_info) => {
                    println!("  Status: Member");
                    println!("  User is a member of the vault");
                    println!("  Vault Name: {}", member_info.member.vault.vault_name);

                    println!();
                    println!("Vault Information:");
                    println!("  Users: {}", member_info.member.vault.users.len());
                    println!(
                        "  Current owner (device id): {:?}",
                        member_info.member.member.user_data.user_id().device_id.id_str()
                    );
                    
                    if !member_info.member.vault.users.is_empty() {
                        println!("  User Details:");
                        let users = member_info.member.vault.users.iter();
                        for (i, (device_id, user)) in users.enumerate() {
                            if let UserMembership::Member(member) = user {
                                println!(
                                    "    Member #{}: Device Id={:?}, Device Name={}",
                                    i + 1,
                                    member.user_data.user_id().device_id.id_str(),
                                    member.user_data.device.device_name.as_str()
                                );
                            } else {
                                println!("    Outsider #{}: ID={:?} ", i + 1, device_id);
                            }
                        }
                    }

                    println!();
                    println!("===== SECRETS_INFO_BEGIN =====");
                    println!("  Total Secrets: {}", member_info.member.vault.secrets.len());
                    if !member_info.member.vault.secrets.is_empty() {
                        println!("  Secret Details:");
                        let secrets = member_info.member.vault.secrets.iter();
                        for (i, secret_id) in secrets.enumerate() {
                            println!("    Secret #{}", i + 1);
                            println!("    Id: {:?}", secret_id.id.clone().id_str());
                            println!("    Name: {:?}", secret_id.name);
                        }
                    }
                    println!("===== SECRETS_INFO_END =====");

                    println!();
                    println!("===== RECOVERY_CLAIMS_INFO_BEGIN =====");
                    if member_info.ss_claims.claims.is_empty() {
                        println!("  No recovery claims available.");
                    } else {
                        println!(
                            "  Total Recovery Claims: {}",
                            member_info.ss_claims.claims.len()
                        );

                        let claims = member_info.ss_claims.claims.iter();
                        for (i, (claim_id, ss_claim)) in claims.enumerate() {
                            println!("  Claim #{}: Id=\"{}\"", i + 1, claim_id.0.clone().id_str());
                            println!("    Sender: {:?}", ss_claim.sender.clone().id_str());
                            println!("    Type: {:?}", ss_claim.distribution_type);
                            println!("    Password: {:?}", ss_claim.dist_claim_id.pass_id.name);
                            println!("    Status: {:?}", ss_claim.status.status());
                            
                            // Display receivers and their statuses
                            if !ss_claim.receivers.is_empty() {
                                println!("    Receivers ({}):", ss_claim.receivers.len());
                                for (j, receiver) in ss_claim.receivers.iter().enumerate() {
                                    let status = ss_claim.status.get(receiver)
                                        .map_or("Unknown", |s| match s {
                                            SsDistributionStatus::Pending => "Pending",
                                            SsDistributionStatus::Sent => "Sent",
                                            SsDistributionStatus::Delivered => "Delivered",
                                        });
                                    println!("      Receiver #{}: {:?} (Status: {})", j + 1, receiver, status);
                                }
                                println!();
                            }
                        }
                    }
                    println!("===== RECOVERY_CLAIMS_INFO_END =====");

                    println!();
                    println!("===== VAULT_EVENTS_INFO_BEGIN =====");
                    if member_info.vault_events.requests.is_empty() {
                        println!("  No pending join requests");
                    } else {
                        println!(
                            "  Pending Join Requests: {}",
                            member_info.vault_events.requests.len()
                        );
                        for (i, request) in member_info.vault_events.requests.iter().enumerate() {
                            match request {
                                VaultActionRequestEvent::JoinCluster(join_request) => {
                                    println!(
                                        "  Request #{}: Device={}, User ID={:?}",
                                        i + 1,
                                        join_request.candidate.device.device_name.as_str(),
                                        join_request.candidate.user_id()
                                    );
                                }
                                VaultActionRequestEvent::AddMetaPass(meta_pass) => {
                                    println!(
                                        "  Request #{}: Meta Pass ID={:?}, Sender={:?}",
                                        i + 1,
                                        meta_pass.meta_pass_id,
                                        meta_pass.sender.user_data.user_id()
                                    );
                                }
                            }
                        }
                    }
                    println!("===== VAULT_EVENTS_INFO_END =====");
                }
            },
        }

        Ok(())
    }
} 