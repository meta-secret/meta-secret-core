//! Join catch-up derives from durable sender material, never a RAM member cursor.
use super::{MetaOrchestrator, join_source::packet};
use crate::node::{
    common::model::{
        device::common::DeviceId,
        secret::SsClaim,
        vault::{vault::VaultStatus, vault_data::VaultData},
    },
    db::{
        events::{
            generic_log_event::{ObjIdExtractor, ToGenericEvent},
            shared_secret_event::SsWorkflowObject,
            vault::vault_log_event::JoinClusterEvent,
        },
        objects::persistent_vault::PersistentVault,
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};
use std::collections::HashSet;

const UNAVAILABLE: &str = "JoinResharingUnavailable";

impl<R: KvLogEventRepo> MetaOrchestrator<R> {
    /// Called only in normal sync success, under the shared client sync owner.
    pub(crate) async fn reconcile_unfinished_join_resharing(&self) -> Result<()> {
        let persistent = PersistentVault::from(self.p_obj.clone());
        let VaultStatus::Member(member) = persistent.find(self.user_creds.user()).await? else {
            return Ok(());
        };
        let vault = self.get_vault(member).await?;
        if !vault.is_member(self.user_creds.device_id()) {
            return Ok(());
        }
        let log = self.get_ss_log_data().await?;
        let mut unfinished = vec![];
        let mut target = None;
        // Preflight before any computation; a different Pass must not hide bad output.
        for pass in &vault.secrets {
            if vault.deleted_secrets.contains(pass) {
                continue;
            }
            if let Some(source) = self.audit_join_source(&vault, pass, &log).await?
                && let Some(missing) = source.missing.first()
            {
                target.get_or_insert_with(|| missing.clone());
                unfinished.push(pass.clone());
            }
        }
        let Some(target) = target else {
            return Ok(()); // Covered Pending/absent is UNKNOWN delivery, not a new split.
        };
        let member = vault
            .members()
            .into_iter()
            .find(|member| member.user_data.device.device_id == target)
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        self.redistribute_existing_secrets(
            &vault,
            &JoinClusterEvent {
                candidate: member.user_data,
                request_id: String::new(),
            },
        )
        .await?;
        let actual = self.get_ss_log_data().await?;
        for pass in unfinished {
            let source = self
                .audit_join_source(&vault, &pass, &actual)
                .await?
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
            // An inherited insufficient-shares `continue` cannot pretend to repair C.
            ensure!(source.missing.is_empty(), UNAVAILABLE);
            ensure!(
                self.join_remotes(&vault) == source.claim.receivers.iter().cloned().collect(),
                UNAVAILABLE
            );
        }
        Ok(())
    }

    pub(super) fn join_remotes(&self, vault: &VaultData) -> HashSet<DeviceId> {
        vault
            .members()
            .into_iter()
            .map(|member| member.user_data.device.device_id)
            .filter(|id| id != self.user_creds.device_id())
            .collect()
    }

    /// No network handoff occurs during compute: every new intended output must exist.
    pub(super) async fn verify_new_join_outputs(
        &self,
        vault: &VaultData,
        claim: &SsClaim,
        intended: &[SsWorkflowObject],
    ) -> Result<()> {
        let log = self.get_ss_log_data().await?;
        ensure!(log.claims.get(&claim.id) == Some(claim), UNAVAILABLE);
        let members: HashSet<_> = vault
            .members()
            .into_iter()
            .map(|member| member.user_data.device.device_id)
            .collect();
        let mut observed = HashSet::new();
        for output in intended {
            let data = output.clone().to_distribution_data()?;
            let receiver = data
                .secret_message
                .cipher_text()
                .channel
                .receiver()
                .to_device_id();
            let data = packet(
                output,
                &vault.vault_name,
                &claim.dist_claim_id.pass_id,
                &receiver,
            )?;
            ensure!(
                data.claim_id == claim.dist_claim_id
                    && members.contains(&receiver)
                    && observed.insert(receiver)
                    && self.p_obj.repo.find_one(output.obj_id()).await?
                        == Some(output.clone().to_generic()),
                UNAVAILABLE
            );
        }
        ensure!(observed == members, UNAVAILABLE);
        let source = self
            .audit_join_source(vault, &claim.dist_claim_id.pass_id, &log)
            .await?
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        ensure!(
            source.claim == *claim
                && source.missing.is_empty()
                && self.join_remotes(vault) == claim.receivers.iter().cloned().collect(),
            UNAVAILABLE
        );
        Ok(())
    }
}
