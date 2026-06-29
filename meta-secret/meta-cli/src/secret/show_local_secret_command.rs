use crate::base_command::BaseCommand;
use crate::cli_format::CliOutputFormat;
use anyhow::{bail, Result};
use meta_secret_core::node::common::model::meta_pass::MetaPasswordId;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use meta_secret_core::node::common::model::secret::SsDistributionId;
use meta_secret_core::recover_from_shares;
use meta_secret_core::secret::shared_secret::UserShareDto;
use serde_json::json;

pub struct ShowLocalSecretCommand {
    base: BaseCommand,
    output_format: CliOutputFormat,
}

impl ShowLocalSecretCommand {
    pub fn new(db_name: String, output_format: CliOutputFormat) -> Self {
        Self {
            base: BaseCommand::new(db_name),
            output_format,
        }
    }

    pub async fn execute(self, pass_name: String) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;

        self.base.ensure_user_creds(&db_context).await?;
        let user_creds = db_context.p_creds.get_user_creds().await?.unwrap();

        let client = self.base.create_client_service(&db_context).await?;
        let app_state = client.get_app_state().await?;

        match app_state {
            ApplicationState::Local(_) => bail!("Local state: operation is not possible."),
            ApplicationState::Vault(vault_full_info) => match vault_full_info {
                VaultFullInfo::NotExists(_) => bail!("Vault does not exist."),
                VaultFullInfo::Outsider(_) => bail!("Outsider: operation is not possible."),
                VaultFullInfo::Member(member_info) => {
                    let pass_id = MetaPasswordId::build(pass_name);
                    let desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
                        pass_id: pass_id.clone(),
                        receiver: user_creds.device_id().clone(),
                    });

                    let dist = db_context
                        .p_obj
                        .find_tail_event(desc)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("Distribution not found for local show"))?
                        .to_distribution_data()?;

                    let transport_sk = &user_creds.device_creds.secret_box.transport.sk;
                    let decrypted = dist.secret_message.cipher_text().decrypt(transport_sk)?;
                    let share = UserShareDto::try_from(&decrypted.msg)?;
                    let secret = recover_from_shares(vec![share])?;

                    match self.output_format {
                        CliOutputFormat::Json => {
                            let result = json!({
                                "secret": secret.text,
                                "status": "success",
                                "password_name": pass_id.name,
                                "devices_count": member_info.member.vault.members().len(),
                            });
                            println!("{}", serde_json::to_string_pretty(&result)?);
                        }
                        CliOutputFormat::Yaml => {
                            println!("secret: {}", secret.text);
                            println!("status: success");
                            println!("password_name: {}", pass_id.name);
                            println!("devices_count: {}", member_info.member.vault.members().len());
                        }
                    }
                }
            },
        }

        Ok(())
    }
}
