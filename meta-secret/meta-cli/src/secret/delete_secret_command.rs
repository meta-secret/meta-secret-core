use crate::base_command::BaseCommand;
use anyhow::{Result, bail};
use dialoguer::Confirm;
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use std::io::{self, IsTerminal};

pub struct DeleteSecretCommand {
    pub base: BaseCommand,
}

impl DeleteSecretCommand {
    pub async fn execute(&self, pass_name: String, confirmed: bool) -> Result<()> {
        if !confirmed {
            if !io::stdin().is_terminal() {
                bail!("Local confirmation is required; use --yes");
            }
            if !Confirm::new()
                .with_prompt(format!("Delete '{pass_name}' from every Vault device?"))
                .default(false)
                .interact()?
            {
                return Ok(());
            }
        }
        eprintln!("Synchronizing Vault and applying pending deletions…");
        let db = self.base.open_existing_db().await?;
        let client = self.base.create_client_service(&db).await?;
        let state = client.get_app_state().await?;
        let pass_id = client.resolve_secret(&pass_name).await?;
        let previous: Vec<_> = match &state {
            ApplicationState::Vault(VaultFullInfo::Member(member)) => member
                .deletion_states
                .iter()
                .filter_map(|s| s.request_id.clone())
                .collect(),
            _ => bail!("User is not a Vault member"),
        };
        let state = client
            .handle_client_request(state, GenericAppStateRequest::DeleteSecret(pass_id))
            .await?;
        let ApplicationState::Vault(VaultFullInfo::Member(member)) = state else {
            bail!("Vault state unavailable");
        };
        let result = member
            .deletion_states
            .iter()
            .find(|s| {
                s.request_id
                    .as_ref()
                    .is_some_and(|id| !previous.contains(id))
            })
            .ok_or_else(|| anyhow::anyhow!("Deletion result is not available"))?;
        println!("{}", serde_json::to_string_pretty(result)?);
        Ok(())
    }

    pub async fn decide(&self, request_id: String, accepted: bool, confirmed: bool) -> Result<()> {
        if accepted && !confirmed {
            if !io::stdin().is_terminal() {
                bail!("Approval confirmation is required; use --yes");
            }
            if !Confirm::new()
                .with_prompt("Approve deleting this Secret from every Vault device?")
                .default(false)
                .interact()?
            {
                return Ok(());
            }
        }
        eprintln!("Synchronizing Vault and applying pending deletions…");
        let db = self.base.open_existing_db().await?;
        let client = self.base.create_client_service(&db).await?;
        let state = client.get_app_state().await?;
        let state = client
            .handle_client_request(
                state,
                GenericAppStateRequest::DecideDeleteSecret {
                    request_id: request_id.clone(),
                    accepted,
                },
            )
            .await?;
        let ApplicationState::Vault(VaultFullInfo::Member(member)) = state else {
            bail!("Vault state unavailable");
        };
        let result = member
            .deletion_states
            .iter()
            .find(|s| s.request_id.as_deref() == Some(&request_id))
            .ok_or_else(|| anyhow::anyhow!("Deletion result is not available"))?;
        println!("{}", serde_json::to_string_pretty(result)?);
        Ok(())
    }

    pub async fn requests(&self) -> Result<()> {
        eprintln!("Synchronizing Vault and applying pending deletions…");
        let db = self.base.open_existing_db().await?;
        let client = self.base.create_client_service(&db).await?;
        let ApplicationState::Vault(VaultFullInfo::Member(member)) = client.get_app_state().await?
        else {
            bail!("User is not a Vault member");
        };
        println!("{}", serde_json::to_string_pretty(&member.deletion_states)?);
        Ok(())
    }
}
