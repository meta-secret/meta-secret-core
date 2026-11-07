//! Canonical later Join ends publication authority, never the original ownership proof.
use super::super::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol};
use crate::node::{
    common::model::{user::common::UserData, vault::vault_data::VaultData},
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor,
            vault_descriptor::{VaultDescriptor, VaultLogDescriptor},
        },
        events::{
            generic_log_event::{KeyExtractor, ObjIdExtractor},
            kv_log_event::KvKey,
            local_guarded_preparation::*,
            object_id::{ArtifactId, Next},
            vault::{
                liveboard_metadata::RemovalResultRecord,
                vault_event::VaultObject,
                vault_log_event::{
                    JoinClusterEvent, JoinResultOutcome, VaultActionEvents,
                    VaultActionRequestEvent, VaultActionUpdateEvent, VaultLogObject,
                },
            },
        },
        objects::{
            local_guarded_preparation::{PersistentGuardedPreparation, UNAVAILABLE},
            persistent_vault::PersistentVault,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};

#[path = "guarded_applied_singleton.rs"]
mod singleton;

impl<R: KvLogEventRepo, S: SyncProtocol> SyncGateway<R, S> {
    pub(super) async fn guarded_current_vault(
        &self,
        object: &LocalGuardedPreparationObject,
    ) -> Result<VaultObject> {
        let vault = PersistentVault::from(self.p_obj.clone())
            .get_vault(object.0.value.owner.vault_name.clone())
            .await?;
        let descriptor = VaultDescriptor::from(object.0.value.owner.vault_name.clone());
        ensure!(
            vault.key() == KvKey::artifact(descriptor.clone().to_obj_desc(), vault.obj_id())
                && vault.obj_id().fqdn == ArtifactId::from(descriptor).fqdn
                && vault.0.value.vault_name == object.0.value.owner.vault_name,
            UNAVAILABLE
        );
        Ok(vault)
    }

    pub(super) async fn retire_applied_after_join(
        &self,
        user: &UserData,
        object: &LocalGuardedPreparationObject,
    ) -> Result<bool> {
        let value = &object.0.value;
        if value.phase == GuardedPreparationPhase::BlockedRetiredCommand {
            ensure!(value.payload.is_none(), UNAVAILABLE);
            return Ok(true); // Monotonic: topology returning to two never reactivates it.
        }
        if !value.origin.as_ref().is_some_and(|origin| origin.accepted) {
            return Ok(false);
        }
        let Some(result) = self.original_own_applied(object).await? else {
            return Ok(false);
        };
        let vault = self.guarded_current_vault(object).await?;
        let members = vault.0.value.members();
        if members.len() == value.survivors.len()
            && members
                .iter()
                .all(|m| value.survivors.contains(&m.user_data))
        {
            return Ok(false);
        }
        if members.len() == 1 {
            self.prove_later_singleton(user, object, &vault, &result)
                .await?;
        } else {
            ensure!(
                value.owner == user.user_id()
                    && value.survivors.len() == 2
                    && value.survivors.contains(user)
                    && members.len() == 3
                    && value
                        .survivors
                        .iter()
                        .all(|u| members.iter().any(|m| m.user_data == *u))
                    && vault.obj_id().fqdn == value.base_vault.fqdn
                    && vault.obj_id().id.curr > value.base_vault.id.curr,
                UNAVAILABLE
            );
            let third = members
                .iter()
                .find(|m| !value.survivors.contains(&m.user_data))
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
            self.prove_later_join(&vault.0.value, &result, &third.user_data)
                .await?;
        }
        let mut terminal = value.clone();
        terminal.payload = None;
        terminal.phase = GuardedPreparationPhase::BlockedRetiredCommand;
        // The fixed replacement performs a whole-object production readback before return.
        PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        }
        .replace(object, terminal)
        .await?;
        Ok(true)
    }

    async fn prove_later_join(
        &self,
        vault: &VaultData,
        applied: &RemovalResultRecord,
        third: &UserData,
    ) -> Result<()> {
        let descriptor = VaultLogDescriptor::from(vault.vault_name.clone());
        let head = PersistentVault::from(self.p_obj.clone())
            .get_vault_log_artifact(vault.vault_name.clone())
            .await?;
        let logs: Vec<VaultLogObject> = self
            .p_obj
            .get_object_events_from_beginning(descriptor.clone())
            .await?;
        let mut expected = ArtifactId::from(descriptor.clone());
        for log in &logs {
            ensure!(
                log.key() == KvKey::artifact(descriptor.clone().to_obj_desc(), expected.clone()),
                UNAVAILABLE
            );
            log.0.value.validate_removal_bindings(true)?;
            expected = expected.next();
        }
        ensure!(logs.last() == Some(&head), UNAVAILABLE);
        let applied_at = logs
            .iter()
            .position(|log| log.0.value.liveboard_removal_results.contains(applied))
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        let mut qualifying = 0;
        for result in &head.0.value.join_results {
            if result.request.candidate != *third
                || result.outcome != JoinResultOutcome::Accepted
                || result.request.request_id.is_empty()
            {
                continue;
            }
            ensure!(
                uuid::Uuid::parse_str(&result.request.request_id).is_ok(),
                UNAVAILABLE
            );
            let mut first = None;
            for (index, log) in logs.iter().enumerate() {
                if join_presence(&log.0.value, &result.request)? {
                    first.get_or_insert(index);
                }
            }
            // Initial accepted joins remain in cumulative snapshots, but cannot end this lifetime.
            if first.is_some_and(|first| first > applied_at) {
                ensure!(
                    !head
                        .0
                        .value
                        .requests
                        .contains(&VaultActionRequestEvent::JoinCluster(
                            result.request.clone()
                        )),
                    UNAVAILABLE
                );
                qualifying += 1;
            }
        }
        ensure!(qualifying == 1, UNAVAILABLE);
        Ok(())
    }
}

fn join_presence(log: &VaultActionEvents, request: &JoinClusterEvent) -> Result<bool> {
    let mut present = false;
    for value in &log.requests {
        if let VaultActionRequestEvent::JoinCluster(actual) = value
            && actual.request_id == request.request_id
        {
            ensure!(actual == request, UNAVAILABLE);
            present = true;
        }
    }
    for value in &log.updates {
        if let VaultActionUpdateEvent::UpdateMembership(update) = value
            && update.request.request_id == request.request_id
        {
            ensure!(
                update.request == *request
                    && matches!(&update.update, crate::node::common::model::user::common::UserMembership::Member(m) if m.user_data == request.candidate),
                UNAVAILABLE
            );
            present = true;
        }
    }
    for result in &log.join_results {
        if result.request.request_id == request.request_id {
            ensure!(
                result.request == *request && result.outcome == JoinResultOutcome::Accepted,
                UNAVAILABLE
            );
            present = true;
        }
    }
    Ok(present)
}
