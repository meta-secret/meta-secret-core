use anyhow::Result;
use meta_secret_core::node::app::meta_app::meta_client_service::{
    MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
};
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::model::user::common::UserMembership;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent;
use std::sync::Arc;
use crate::base_command::BaseCommand;

pub struct InfoCommand {
    base: BaseCommand,
}

impl InfoCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
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
                "Run the 'meta-secret init-user --vault-name <name>' command to associate it with a vault."
            );
            return Ok(());
        };

        println!("User Information:");
        println!("  User ID: {:?}", user_creds.user_id());
        println!("  Vault Name: {}", user_creds.vault_name);
        println!();

        let sync_protocol = HttpSyncProtocol {
            api_url: ApiUrl::prod(),
        };

        let device_creds = Arc::new(user_creds.device_creds.clone());
        let sync_gateway = Arc::new(SyncGateway {
            id: "meta-cli".to_string(),
            p_obj: db_context.p_obj.clone(),
            sync: Arc::new(sync_protocol),
            device_creds: device_creds.clone(),
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());

        let client = MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer {
                dt: MpscDataTransfer::new(),
            }),
            sync_gateway,
            state_provider,
            p_obj: db_context.p_obj,
            device_creds,
        };

        println!("Syncing with server to get latest state...");
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
                    println!(
                        "  Owner: {:?}",
                        member_info.member.member.user_data.user_id()
                    );

                    println!("  Users: {}", member_info.member.vault.users.len());
                    if !member_info.member.vault.users.is_empty() {
                        println!("  User Details:");
                        for (i, (device_id, user)) in
                            member_info.member.vault.users.iter().enumerate()
                        {
                            if let UserMembership::Member(member) = user {
                                println!(
                                    "    User #{}: ID={:?}, Device={}",
                                    i + 1,
                                    member.user_data.user_id(),
                                    member.user_data.device.device_name.as_str()
                                );
                            } else {
                                println!("    User #{}: ID={:?} (not a member)", i + 1, device_id);
                            }
                        }
                    }

                    println!("  Secrets: {}", member_info.member.vault.secrets.len());
                    if !member_info.member.vault.secrets.is_empty() {
                        println!("  Secret Details:");
                        for (i, secret_id) in member_info.member.vault.secrets.iter().enumerate() {
                            println!("    Secret #{}: ID={:?}", i + 1, secret_id);
                        }
                    }

                    println!();
                    println!("Shared secret claims:");
                    if member_info.ss_claims.claims.is_empty() {
                        println!("  No secret claims.");
                    } else {
                        println!(
                            "  Number of Shared Secret claims: {}",
                            member_info.ss_claims.claims.len()
                        );
                        for (i, (claim_id, ss_claim)) in
                            member_info.ss_claims.claims.iter().enumerate()
                        {
                            println!(
                                "  Share #{}: ID={:?}, Status={:?}",
                                i + 1,
                                claim_id,
                                ss_claim.status
                            );
                        }
                    }

                    println!();
                    println!("Vault Actions:");
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
                }
            },
        }

        Ok(())
    }
}
