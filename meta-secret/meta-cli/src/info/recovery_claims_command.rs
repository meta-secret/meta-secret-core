use crate::cli_format::CliOutputFormat;
use crate::info::info_command_base::{InfoCommandBase, InfoCommandTrait};
use crate::template_manager::TemplateManager;
use anyhow::Result;
use meta_secret_core::node::common::model::secret::SsDistributionStatus;
use meta_secret_core::node::common::model::{IdString, VaultFullInfo, ApplicationState};
use serde_json::json;
use tera::Context;

// Command for showing recovery claims
pub struct RecoveryClaimsInfoCommand {
    base: InfoCommandBase,
}

impl RecoveryClaimsInfoCommand {
    pub fn new(db_name: String, output_format: CliOutputFormat) -> Self {
        Self {
            base: InfoCommandBase::new(db_name, output_format),
        }
    }
}

impl InfoCommandTrait for RecoveryClaimsInfoCommand {
    async fn execute(&self) -> Result<()> {
        let app_state = self.base.get_app_state().await?;
        let mut context = Context::new();

        match app_state {
            ApplicationState::Vault(VaultFullInfo::Member(member_info)) => {
                if member_info.ss_claims.claims.is_empty() {
                    // Empty context will result in empty claims array
                    let output = TemplateManager::instance().render(
                        "recovery_claims",
                        &context,
                        self.base.output_format(),
                    )?;
                    println!("{}", output);
                    return Ok(());
                }

                let mut claims_vec = Vec::new();
                for (claim_id, ss_claim) in &member_info.ss_claims.claims {
                    let mut receivers = Vec::new();
                    for receiver in &ss_claim.receivers {
                        let status = ss_claim
                            .status
                            .get(receiver)
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

                    claims_vec.push(json!({
                        "id": claim_id.0.clone().id_str(),
                        "sender": ss_claim.sender.clone().id_str(),
                        "type": format!("{:?}", ss_claim.distribution_type),
                        "password": ss_claim.dist_claim_id.pass_id.name.clone(),
                        "status": format!("{:?}", ss_claim.status.status()),
                        "receivers": receivers
                    }));
                }

                context.insert("claims", &claims_vec);
                let output = TemplateManager::instance().render(
                    "recovery_claims",
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