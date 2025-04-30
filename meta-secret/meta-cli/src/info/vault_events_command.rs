use crate::cli_format::CliOutputFormat;
use crate::info::info_command_base::{InfoCommandBase, InfoCommandTrait};
use crate::template_manager::TemplateManager;
use anyhow::Result;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent;
use serde_json::json;
use tera::Context;

// Command for showing vault events
pub struct VaultEventsInfoCommand {
    base: InfoCommandBase,
}

impl VaultEventsInfoCommand {
    pub fn new(db_name: String, output_format: CliOutputFormat) -> Self {
        Self {
            base: InfoCommandBase::new(db_name, output_format),
        }
    }
}

impl InfoCommandTrait for VaultEventsInfoCommand {
    async fn execute(&self) -> Result<()> {
        let app_state = self.base.get_app_state().await?;
        let mut context = Context::new();

        match app_state {
            ApplicationState::Vault(VaultFullInfo::Member(member_info)) => {
                if member_info.vault_events.requests.is_empty() {
                    // Empty context will result in empty events array
                    let output = TemplateManager::instance().render(
                        "vault_events",
                        &context,
                        self.base.output_format(),
                    )?;
                    println!("{}", output);
                    return Ok(());
                }

                let mut events_vec = Vec::new();
                for request in &member_info.vault_events.requests {
                    match request {
                        VaultActionRequestEvent::JoinCluster(join_request) => {
                            events_vec.push(json!({
                                "type": "JoinCluster",
                                "device_name": join_request.candidate.device.device_name.as_str().to_string(),
                                "user_id": format!("{:?}", join_request.candidate.user_id())
                            }));
                        }
                        VaultActionRequestEvent::AddMetaPass(meta_pass) => {
                            events_vec.push(json!({
                                "type": "AddMetaPass",
                                "meta_pass_id": format!("{:?}", meta_pass.meta_pass_id),
                                "sender": format!("{:?}", meta_pass.sender.user_data.user_id())
                            }));
                        }
                    }
                }

                context.insert("events", &events_vec);
                let output = TemplateManager::instance().render(
                    "vault_events",
                    &context,
                    self.base.output_format(),
                )?;
                println!("{}", output);
            }
            _ => {
                let mut error_context = Context::new();
                error_context.insert("message", "Not a vault member or vault doesn't exist");
                let output = TemplateManager::instance().render(
                    "error",
                    &error_context,
                    self.base.output_format(),
                )?;
                println!("{}", output);
            }
        }

        Ok(())
    }
} 