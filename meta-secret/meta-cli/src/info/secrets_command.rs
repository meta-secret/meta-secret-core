use crate::cli_format::CliOutputFormat;
use crate::info::info_command_base::{InfoCommandBase, InfoCommandTrait};
use crate::template_manager::TemplateManager;
use anyhow::Result;
use meta_secret_core::node::common::model::{ApplicationState, IdString, VaultFullInfo};
use serde_json::json;
use tera::Context;

// Command for showing secrets
pub struct SecretsInfoCommand {
    base: InfoCommandBase,
}

impl SecretsInfoCommand {
    pub fn new(db_name: String, output_format: CliOutputFormat) -> Self {
        Self {
            base: InfoCommandBase::new(db_name, output_format),
        }
    }
}

impl InfoCommandTrait for SecretsInfoCommand {
    async fn execute(&self) -> Result<()> {
        let app_state = self.base.get_app_state().await?;
        let mut context = Context::new();

        match app_state {
            ApplicationState::Vault(VaultFullInfo::Member(member_info)) => {
                if member_info.member.vault.secrets.is_empty() {
                    // Empty context will result in empty secrets array
                    let output = TemplateManager::instance().render(
                        "secrets",
                        &context,
                        self.base.output_format(),
                    )?;
                    println!("{}", output);
                    return Ok(());
                }

                let mut secrets_vec = Vec::new();
                for secret_id in &member_info.member.vault.secrets {
                    secrets_vec.push(json!({
                        "id": secret_id.id.clone().id_str(),
                        "name": secret_id.name.clone()
                    }));
                }

                context.insert("secrets", &secrets_vec);
                let output = TemplateManager::instance().render(
                    "secrets",
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
