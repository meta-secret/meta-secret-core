use crate::cli_format::CliOutputFormat;
use crate::info::info_command_base::{InfoCommandBase, InfoCommandTrait};
use crate::template_manager::TemplateManager;
use anyhow::Result;
use meta_secret_core::node::common::model::secret::SsDistributionStatus;
use meta_secret_core::node::common::model::user::common::UserMembership;
use meta_secret_core::node::common::model::{ApplicationState, IdString, VaultFullInfo};
use meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent;
use serde_json::json;
use tera::Context;

// Command for showing default info
pub struct DefaultInfoCommand {
    base: InfoCommandBase,
}

impl DefaultInfoCommand {
    pub fn new(db_name: String, output_format: CliOutputFormat) -> Self {
        Self {
            base: InfoCommandBase::new(db_name, output_format),
        }
    }
}

impl InfoCommandTrait for DefaultInfoCommand {
    async fn execute(&self) -> Result<()> {
        let db_context = self.base.base().open_existing_db().await?;
        let mut context = Context::new();

        // Try to get device credentials
        let maybe_device_creds = db_context.p_creds.get_device_creds().await?;

        if let Some(device_creds_event) = maybe_device_creds {
            let device_creds = device_creds_event.value();
            context.insert(
                "device",
                &json!({
                    "id": device_creds.device.device_id,
                    "name": device_creds.device.device_name.as_str()
                }),
            );
        } else {
            // Just render the template with the current context to show initialization message
            let output =
                TemplateManager::instance().render("info", &context, self.base.output_format())?;
            print!("{}", output);
            return Ok(());
        }

        let maybe_user_creds = db_context.p_creds.get_user_creds().await?;

        if let Some(user_creds) = maybe_user_creds {
            context.insert(
                "user",
                &json!({
                    "vault_name": user_creds.vault_name
                }),
            );
        } else {
            // Just render the template with the current context to show the "no user" message
            let output =
                TemplateManager::instance().render("info", &context, self.base.output_format())?;
            print!("{}", output);
            return Ok(());
        }

        // Get app state using client service
        let client = self.base.base().create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        match app_state {
            ApplicationState::Local(device_data) => {
                context.insert(
                    "app_state",
                    &json!({
                        "status": "Local",
                        "device_id": device_data.device_id,
                    }),
                );
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(user_data) => {
                    context.insert(
                        "app_state",
                        &json!({
                            "status": "Vault not exists",
                            "vault_name": user_data.vault_name(),
                        }),
                    );
                }
                VaultFullInfo::Outsider(outsider) => {
                    context.insert(
                        "app_state",
                        &json!({
                            "status": "Outsider",
                            "vault_name": outsider.user_data.vault_name(),
                        }),
                    );
                }
                VaultFullInfo::Member(member_info) => {
                    // Build structured data for the template
                    let mut users = Vec::new();
                    for (_i, (device_id, user)) in member_info.member.vault.users.iter().enumerate()
                    {
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
                            let status =
                                ss_claim
                                    .status
                                    .get(receiver)
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
                            }
                            VaultActionRequestEvent::AddMetaPass(meta_pass) => {
                                events.push(json!({
                                    "type": "AddMetaPass",
                                    "meta_pass_id": format!("{:?}", meta_pass.meta_pass_id),
                                    "sender": format!("{:?}", meta_pass.sender.user_data.user_id()),
                                }));
                            }
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

        // Render the template using the template manager
        let output =
            TemplateManager::instance().render("info", &context, self.base.output_format())?;
        print!("{}", output);

        Ok(())
    }
}
