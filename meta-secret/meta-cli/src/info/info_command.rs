use crate::base_command::BaseCommand;
use anyhow::Result;
use meta_secret_core::node::common::model::user::common::UserMembership;
use meta_secret_core::node::common::model::{ApplicationState, IdString, VaultFullInfo};
use meta_secret_core::node::common::model::secret::SsDistributionStatus;
use meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent;
use serde_json::json;
use tera::{Tera, Context};

pub struct InfoCommand {
    base: BaseCommand,
}

impl InfoCommand {
    pub fn new(db_name: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
        }
    }

    /// Helper method to initialize client and get application state
    async fn get_app_state(&self) -> Result<ApplicationState> {
        let db_context = self.base.open_existing_db().await?;
        let client = self.base.create_client_service(&db_context).await?;
        client.get_app_state().await
    }

    /// Shows recovery claims information in JSON format
    pub async fn show_recovery_claims(&self) -> Result<()> {
        let app_state = self.get_app_state().await?;

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
        let app_state = self.get_app_state().await?;

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
        let app_state = self.get_app_state().await?;

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
        let mut tera = Tera::default();
        
        // Define all templates as structured documents
        tera.add_raw_template("main", r#"
Meta Secret Information:
------------------------

{% if device %}
Device Information:
  Device ID: {{ device.id }}
  Device Name: {{ device.name }}
{% else %}
Not initialized. Run the 'meta-secret init-device' command first.
{% endif %}

{% if user %}
User Information:
  Vault Name: {{ user.vault_name }}
{% else %}
User Status: Device is initialized but not associated with a vault.
Run the 'meta-secret init-user --vault-name <n>' command to associate it with a vault.
{% endif %}

{% if app_state %}
Syncing with server to get latest state...

Application State:
  Status: {{ app_state.status }}
  {% if app_state.status == "Local" %}
  Device is initialized but not connected to a vault
  Device ID: {{ app_state.device_id }}
  {% elif app_state.status == "Vault not exists" %}
  User has created credentials but the vault doesn't exist yet
  Vault Name: {{ app_state.vault_name }}
  {% elif app_state.status == "Outsider" %}
  User is not a member of the vault
  Vault Name: {{ app_state.vault_name }}
  User needs to be invited to join the vault
  {% elif app_state.status == "Member" %}
  User is a member of the vault
  Vault Name: {{ app_state.vault_name }}

  {% if app_state.vault %}
  Vault Information:
  Users: {{ app_state.vault.users | length }}
  Current owner (device id): {{ app_state.vault.owner_id }}
  
  {% if app_state.vault.users | length > 0 %}
  User Details:
  {% for user in app_state.vault.users %}
    {% if user.type == "Member" %}
    Member #{{ loop.index }}: Device Id={{ user.device_id }}, Device Name={{ user.device_name }}
    {% else %}
    Outsider #{{ loop.index }}: ID={{ user.device_id }}
    {% endif %}
  {% endfor %}
  {% endif %}
  
  ===== SECRETS_INFO_BEGIN =====
  Total Secrets: {{ app_state.vault.secrets | length }}
  {% if app_state.vault.secrets | length > 0 %}
  Secret Details:
  {% for secret in app_state.vault.secrets %}
    Secret #{{ loop.index }}
    Id: {{ secret.id }}
    Name: {{ secret.name }}
  {% endfor %}
  {% endif %}
  ===== SECRETS_INFO_END =====
  
  ===== RECOVERY_CLAIMS_INFO_BEGIN =====
  {% if app_state.recovery_claims | length == 0 %}
  No recovery claims available.
  {% else %}
  Total Recovery Claims: {{ app_state.recovery_claims | length }}
  {% for claim in app_state.recovery_claims %}
  Claim #{{ loop.index }}: Id="{{ claim.id }}"
    Sender: {{ claim.sender }}
    Type: {{ claim.type }}
    Password: {{ claim.password }}
    Status: {{ claim.status }}
    
    {% if claim.receivers | length > 0 %}
    Receivers ({{ claim.receivers | length }}):
    {% for receiver in claim.receivers %}
      Receiver #{{ loop.index }}: {{ receiver.id }} (Status: {{ receiver.status }})
    {% endfor %}
    
    {% endif %}
  {% endfor %}
  {% endif %}
  ===== RECOVERY_CLAIMS_INFO_END =====
  
  ===== VAULT_EVENTS_INFO_BEGIN =====
  {% if app_state.vault.events | length == 0 %}
  No pending join requests
  {% else %}
  Pending Join Requests: {{ app_state.vault.events | length }}
  {% for event in app_state.vault.events %}
  {% if event.type == "JoinCluster" %}
  Request #{{ loop.index }}: Device={{ event.device }}, User ID={{ event.user_id }}
  {% elif event.type == "AddMetaPass" %}
  Request #{{ loop.index }}: Meta Pass ID={{ event.meta_pass_id }}, Sender={{ event.sender }}
  {% endif %}
  {% endfor %}
  {% endif %}
  ===== VAULT_EVENTS_INFO_END =====
  {% endif %}
  {% endif %}
{% endif %}
"#)?;

        let mut context = Context::new();
        
        // Try to get device credentials
        let maybe_device_creds = db_context.p_creds.get_device_creds().await?;

        if let Some(device_creds_event) = maybe_device_creds {
            let device_creds = device_creds_event.value();
            context.insert("device", &json!({
                "id": device_creds.device.device_id,
                "name": device_creds.device.device_name.as_str()
            }));
        } else {
            print!("{}", tera.render("main", &context)?);
            return Ok(());
        }

        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;
        
        if let Some(user_creds) = maybe_user_creds {
            context.insert("user", &json!({
                "vault_name": user_creds.vault_name
            }));
        } else {
            print!("{}", tera.render("main", &context)?);
            return Ok(());
        }

        // Get app state using client service
        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;
        
        match app_state {
            ApplicationState::Local(device_data) => {
                context.insert("app_state", &json!({
                    "status": "Local",
                    "device_id": device_data.device_id,
                }));
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(user_data) => {
                    context.insert("app_state", &json!({
                        "status": "Vault not exists",
                        "vault_name": user_data.vault_name(),
                    }));
                }
                VaultFullInfo::Outsider(outsider) => {
                    context.insert("app_state", &json!({
                        "status": "Outsider",
                        "vault_name": outsider.user_data.vault_name(),
                    }));
                }
                VaultFullInfo::Member(member_info) => {
                    // Build structured data for the template
                    let mut users = Vec::new();
                    for (_i, (device_id, user)) in member_info.member.vault.users.iter().enumerate() {
                        if let UserMembership::Member(member) = user {
                            users.push(json!({
                                "type": "Member",
                                "device_id": member.user_data.user_id().device_id.id_str(),
                                "device_name": member.user_data.device.device_name.as_str(),
                            }));
                        } else {
                            users.push(json!({
                                "type": "Outsider",
                                "device_id": device_id,
                            }));
                        }
                    }
                    
                    let mut secrets = Vec::new();
                    for secret_id in &member_info.member.vault.secrets {
                        secrets.push(json!({
                            "id": secret_id.id.clone().id_str(),
                            "name": secret_id.name,
                        }));
                    }
                    
                    let mut claims = Vec::new();
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
                                "id": receiver,
                                "status": status
                            }));
                        }
                        
                        claims.push(json!({
                            "id": claim_id.0.clone().id_str(),
                            "sender": ss_claim.sender.clone().id_str(),
                            "type": format!("{:?}", ss_claim.distribution_type),
                            "password": ss_claim.dist_claim_id.pass_id.name,
                            "status": format!("{:?}", ss_claim.status.status()),
                            "receivers": receivers
                        }));
                    }
                    
                    let mut events = Vec::new();
                    for request in &member_info.vault_events.requests {
                        match request {
                            VaultActionRequestEvent::JoinCluster(join_request) => {
                                events.push(json!({
                                    "type": "JoinCluster",
                                    "device": join_request.candidate.device.device_name.as_str(),
                                    "user_id": format!("{:?}", join_request.candidate.user_id()),
                                }));
                            },
                            VaultActionRequestEvent::AddMetaPass(meta_pass) => {
                                events.push(json!({
                                    "type": "AddMetaPass",
                                    "meta_pass_id": format!("{:?}", meta_pass.meta_pass_id),
                                    "sender": format!("{:?}", meta_pass.sender.user_data.user_id()),
                                }));
                            },
                        }
                    }
                    
                    context.insert("app_state", &json!({
                        "status": "Member",
                        "vault_name": member_info.member.vault.vault_name,
                        "vault": {
                            "users": users,
                            "owner_id": member_info.member.member.user_data.user_id().device_id.id_str(),
                            "secrets": secrets,
                            "events": events,
                        },
                        "recovery_claims": claims,
                    }));
                }
            },
        }
        
        print!("{}", tera.render("main", &context)?);
        Ok(())
    }
} 