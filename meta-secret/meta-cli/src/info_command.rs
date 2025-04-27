use std::path::Path;
use std::sync::Arc;
use anyhow::{bail, Result};
use meta_db_redb::ReDbRepo;
use meta_secret_core::node::app::meta_app::meta_client_service::{MetaClientDataTransfer, MetaClientService, MetaClientStateProvider};
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;

pub struct InfoCommand {
    pub db_name: String,
}

impl InfoCommand {
    pub async fn execute(&self) -> Result<()> {
        // Database file path
        let db_path = Path::new(self.db_name.as_str());
        
        if !db_path.exists() {
            println!("No database found. Please run 'meta-secret init-device' command first.");
            bail!("Database not found");
        }
        
        // Open existing database
        let repo = Arc::new(ReDbRepo::open(db_path)?);
        let p_obj = Arc::new(PersistentObject::new(repo));
        let p_creds = PersistentCredentials { p_obj: p_obj.clone() };
        
        println!("Meta Secret Information:");
        println!("------------------------");
        
        // Try to get device credentials
        let maybe_device_creds = p_creds.get_device_creds().await?;
        
        match maybe_device_creds {
            Some(device_creds_event) => {
                let device_creds = device_creds_event.value();
                println!("Device Information:");
                println!("  Device ID: {}", device_creds.device.device_id);
                println!("  Device Name: {}", device_creds.device.device_name.as_str());
            }
            None => {
                println!("Not initialized. Run the 'meta-secret init-device' command first.");
                return Ok(());
            }
        }
        
        let maybe_user_creds = p_creds.get_user_creds().await?;
        
        println!();
        let Some(user_creds) = maybe_user_creds else {
            println!("User Status: Device is initialized but not associated with a vault.");
            println!("Run the 'meta-secret init-user --vault-name <name>' command to associate it with a vault.");
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
            p_obj: p_obj.clone(),
            sync: Arc::new(sync_protocol),
            device_creds: device_creds.clone()
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());
        
        let client = MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer { dt: MpscDataTransfer::new() }),
            sync_gateway,
            state_provider,
            p_obj,
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
            ApplicationState::Vault(vault_info) => {
                match vault_info {
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
                        println!("  Owner: {:?}", member_info.member.member.user_data.user_id());
                        println!("  Users: {}", member_info.member.vault.users.len());
                        println!("  Secrets: {}", member_info.member.vault.secrets.len());
                        
                        println!();
                        println!("Shared secret claims:");
                        if member_info.ss_claims.claims.is_empty() {
                            println!("  No secret claims.");
                        } else {
                            println!("  Number of Shared Secret claims: {}", member_info.ss_claims.claims.len());
                            for (i, (claim_id, ss_claim)) in member_info.ss_claims.claims.iter().enumerate() {
                                println!("  Share #{}: ID={:?}, Status={:?}", i+1, claim_id, ss_claim.status);
                            }
                        }
                        
                        println!();
                        println!("Vault Actions:");
                        if member_info.vault_events.join_requests.is_empty() {
                            println!("  No pending join requests");
                        } else {
                            println!("  Pending Join Requests: {}", member_info.vault_events.join_requests.len());
                            for (i, request) in member_info.vault_events.join_requests.iter().enumerate() {
                                println!("  Request #{}: Device={}, User={:?}", 
                                    i+1, 
                                    request.user_data.device.device_name.as_str(),
                                    request.user_data.user_id());
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
} 